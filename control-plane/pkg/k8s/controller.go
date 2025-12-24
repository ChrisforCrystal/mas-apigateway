package k8s

import (
	"context"
	"fmt"
	"log"
	"time"

	agwv1 "github.com/masallsome/masapigateway/control-plane/pkg/proto"
	corev1 "k8s.io/api/core/v1"
	discoveryv1 "k8s.io/api/discovery/v1"
	"k8s.io/apimachinery/pkg/apis/meta/v1/unstructured"
	"k8s.io/apimachinery/pkg/labels"
	"k8s.io/apimachinery/pkg/runtime/schema"
	"k8s.io/client-go/dynamic"
	"k8s.io/client-go/dynamic/dynamicinformer"
	"k8s.io/client-go/informers"
	"k8s.io/client-go/kubernetes"
	"k8s.io/client-go/tools/cache"
)

// Controller 负责监听 Kubernetes 资源的变化并将最新的状态同步到 Registry 中。
// 它是连接 Kubernetes 集群状态和网关内部配置状态的桥梁。
type Controller struct {
	client      *kubernetes.Clientset                // 标准 K8s 客户端，用于访问 Core 资源（如 Service）
	dynClient   dynamic.Interface                    // 动态 K8s 客户端，用于访问 CRD 资源（如 GatewayRoute）
	factory     informers.SharedInformerFactory      // 标准资源的 Informer 工厂，统一管理 Service/EndpointSlice 的监听
	dynFactory  dynamicinformer.DynamicSharedInformerFactory // 动态资源的 Informer 工厂，统一管理 CRD 的监听
	serviceInf  cache.SharedIndexInformer            // Service 资源的监听器
	sliceInf    cache.SharedIndexInformer            // EndpointSlice 资源的监听器（用于获取 Pod IP）
	registry    *Registry                            // 内部服务注册中心，Controller 将 K8s 的变化转换后更新到这里
	routeLister cache.GenericLister                  // GatewayRoute 的 Lister，用于从本地缓存中快速查询路由规则
	routeSynced cache.InformerSynced                 // 一个函数，用于检查 GatewayRoute 的缓存是否已经同步完成
}

// NewController 初始化一个新的控制器实例。
// 它负责装配所有的 "情报系统"：
// 1. 创建共享 Informer 工厂 (Factory)
// 2. 从工厂中获取特定资源的 Informer (Service, EndpointSlice)
// 3. 配置动态客户端以监听自定义资源 (GatewayRoute)
// 4. 注册事件回调函数 (Add/Update/Delete)
func NewController(client *kubernetes.Clientset, dynClient dynamic.Interface, registry *Registry) *Controller {
	// 创建标准资源的 SharedInformerFactory。
	// 30*time.Second 是 "Resync Period" (重新同步周期)。
	// 即使没有变更，Informer 也会每隔 30秒 强制触发一次 Update 事件，确保本地缓存和 Registry 不会因为漏掉事件而永久不一致。
	factory := informers.NewSharedInformerFactory(client, 30*time.Second) 
	
	// 从工厂获取 "外勤特工" (Informer)
	// serviceInf: 监听 Core/V1 下的 Service 资源
	serviceInf := factory.Core().V1().Services().Informer()
	// sliceInf: 监听 Discovery/V1 下的 EndpointSlice 资源 (比旧的 Endpoints 性能更好)
	sliceInf := factory.Discovery().V1().EndpointSlices().Informer()

	// 配置动态 Informer 以监听 GatewayRoute CRD
	// 因为是自定义资源，必须指定 GVR (Group, Version, Resource) 坐标
	gvr := schema.GroupVersionResource{
		Group:    "agw.masallsome.io",
		Version:  "v1",
		Resource: "gatewayroutes",
	}
	// 创建动态资源的 SharedInformerFactory
	dynFactory := dynamicinformer.NewDynamicSharedInformerFactory(dynClient, 30*time.Second)
	// 获取 GatewayRoute 的 Informer 和 Lister
	routeInf := dynFactory.ForResource(gvr).Informer()
	routeLister := dynFactory.ForResource(gvr).Lister()

	// 组装 Controller 结构体
	c := &Controller{
		client:      client,
		dynClient:   dynClient,
		factory:     factory,
		dynFactory:  dynFactory,
		serviceInf:  serviceInf,
		sliceInf:    sliceInf,
		registry:    registry,
		routeLister: routeLister,
		routeSynced: routeInf.HasSynced,
	}

	// 注册 Service 变更的事件回调
	// 当 K8s 中 Service 发生增删改时，触发 c.onServiceXXX 方法
	serviceInf.AddEventHandler(cache.ResourceEventHandlerFuncs{
		AddFunc:    c.onServiceAdd,
		UpdateFunc: c.onServiceUpdate,
		DeleteFunc: c.onServiceDelete,
	})

	// 注册 EndpointSlice 变更的事件回调
	// 这是感知 Pod IP 变化的核心机制
	sliceInf.AddEventHandler(cache.ResourceEventHandlerFuncs{
		AddFunc:    c.onSliceAdd,
		UpdateFunc: c.onSliceUpdate,
		DeleteFunc: c.onSliceDelete,
	})

	// 注册 GatewayRoute (CRD) 变更的事件回调
	// 任何路由规则的变化都会触发 rebuildRoutes，全量重新计算路由表
	routeInf.AddEventHandler(cache.ResourceEventHandlerFuncs{
		AddFunc:    func(obj interface{}) { c.rebuildRoutes() },
		UpdateFunc: func(old, new interface{}) { c.rebuildRoutes() },
		DeleteFunc: func(obj interface{}) { c.rebuildRoutes() },
	})
	
	return c
}

// Run 启动控制器的主要循环。
// 该方法会一直阻塞，直到 ctx 被取消（程序退出）。
func (c *Controller) Run(ctx context.Context) {
	log.Println("Starting K8s Discovery Controller...")
	
	// 启动 Informer 工厂（在后台开始缓存数据）
	// 这里分别启动了标准资源 (Service, EndpointSlice) 和 动态资源 (GatewayRoute) 的监听工厂
	go c.factory.Start(ctx.Done())
	go c.dynFactory.Start(ctx.Done())
	
	// 等待所有的 Informer 缓存同步完成
	// 这是为了确保在处理事件之前，我们已经拥有了集群的完整初始状态
	// 如果超时（通常说明 API Server 连不上），则退出
	if !cache.WaitForCacheSync(ctx.Done(), c.serviceInf.HasSynced, c.sliceInf.HasSynced, c.routeSynced) {
		log.Println("Timed out waiting for caches to sync")
		return
	}
	
	// 定义 GatewayRoute 的 GVR (Group, Version, Resource) 坐标
	// 因为它是 CRD，Go 客户端代码中没有它的强类型定义，所以需要使用 Dynamic Client + GVR 来访问
	gvr := schema.GroupVersionResource{
		Group:    "agw.masallsome.io",
		Version:  "v1",
		Resource: "gatewayroutes",
	}

	// 创建一个专门用于 GatewayRoute 的 Dynamic Informer
	// dynamicinformer 允许我们像监听原生资源一样监听 CRD
	dynInformer := dynamicinformer.NewDynamicSharedInformerFactory(c.dynClient, 0)
	informer := dynInformer.ForResource(gvr).Informer()

	// 注册事件处理函数
	// 无论是新增、更新还是删除 GatewayRoute，我们都触发 rebuildRoutes()
	// rebuildRoutes 会全量重新计算路由表并推送到 Registry
	informer.AddEventHandler(cache.ResourceEventHandlerFuncs{
		AddFunc: func(obj interface{}) {
			c.rebuildRoutes()
		},
		UpdateFunc: func(old, new interface{}) {
			c.rebuildRoutes()
		},
		DeleteFunc: func(obj interface{}) {
			c.rebuildRoutes()
		},
	})
	
	log.Println("Starting GatewayRoute Watcher...")
	// 启动并等待这个特定的 CRD informer 同步
	dynInformer.Start(ctx.Done())
	cache.WaitForCacheSync(ctx.Done(), informer.HasSynced)
	
	// 此时所有 Watcher 都在后台运行，主线程可以通过 <-ctx.Done() 阻塞（在 main.go 调用处体现）
}

// rebuildRoutes 遍历所有缓存中的 GatewayRoute CRD，解析并重新构建路由表。
// 这是一个 "世界重构 (World Rebuild)" 的过程：只要有任何一个路由发生变化，
// 我们就重新扫描所有路由，生成最新的全量路由列表。
func (c *Controller) rebuildRoutes() {
	// 1. 从本地缓存 (Lister) 中获取所有的 GatewayRoute 对象
	objs, err := c.routeLister.List(labels.Everything())
	if err != nil {
		log.Printf("Error listing GatewayRoutes: %v", err)
		return
	}

	var routes []*agwv1.Route
	for _, obj := range objs {
		// 2. 类型断言：因为是 Dynamic Client，拿到的对象是 *unstructured.Unstructured
		// 它本质上是一个 map[string]interface{}，用来存储未知的 CRD 数据结构
		u, ok := obj.(*unstructured.Unstructured)
		if !ok {
			continue
		}
		
		// 3. 解析单个 CRD 对象
		route := c.parseRoute(u)
		if route != nil {
			routes = append(routes, route)
		}
	}

	log.Printf("Rebuilt %d GatewayRoutes from CRDs", len(routes))
	// 4. 将解析好的路由列表更新到 Registry，并触发推送
	c.registry.StoreCRDRoutes(routes)
}

// parseRoute 解析单个 GatewayRoute CRD 对象。
//
// 假设你的 CRD YAML 是长这样的：
// ----------------------------------------
// apiVersion: agw.masallsome.io/v1
// kind: GatewayRoute
// metadata:
//   name: my-route
//   namespace: default
// spec:
//   match: /api/v1  <-- 对应 NestedString(spec, "match")
//   backend:        <-- 对应 NestedMap(spec, "backend")
//     service_name: my-service
//   plugins:        <-- 对应 parsePlugins
//     - name: deny-all
//       wasm_path: /etc/wasm/deny.wasm
// ----------------------------------------
func (c *Controller) parseRoute(u *unstructured.Unstructured) *agwv1.Route {
	// u.Object 就是整个 YAML 的 map[string]interface{} 表示
	
	// 1. 提取 "spec" 字段
	spec, found, _ := unstructured.NestedMap(u.Object, "spec")
	if !found {
		return nil
	}

	// 2. 提取 "spec.match" 字段 (作为路径前缀)
	match, _, _ := unstructured.NestedString(spec, "match")
	if match == "" {
		return nil
	}

	// 3. 提取 "spec.backend.service_name"
	// 这里我们需要先拿到 backend 这个 map，再从里面拿 service_name
	backend, _, _ := unstructured.NestedMap(spec, "backend")
	svcName, _, _ := unstructured.NestedString(backend, "service_name")
	
	if svcName == "" {
		return nil // Invalid route
	}

	// 4. 构建 Cluster ID
	// 格式必须与 Controller 中 processSlice 生成的 Cluster Name 一致: k8s/{ns}/{svc}
	clusterName := fmt.Sprintf("k8s/%s/%s", u.GetNamespace(), svcName)
	
	// 5. 解析插件配置
	plugins := c.parsePlugins(spec)

	return &agwv1.Route{
		PathPrefix: match,
		ClusterId:  clusterName,
		Plugins:    plugins,
	}
}

// parsePlugins 解析插件配置列表
// 对应 YAML:
// spec:
//   plugins:
//     - name: "deny-all"
//       wasm_path: "..."
//       config:
//         key: "value"
func (c *Controller) parsePlugins(spec map[string]interface{}) []*agwv1.Plugin {
	// 1. 提取 "spec.plugins" 列表
	rawPlugins, found, _ := unstructured.NestedSlice(spec, "plugins")
	if !found {
		return nil
	}

	var plugins []*agwv1.Plugin
	for _, p := range rawPlugins {
		// 每个插件项也是一个 map
		pmap, ok := p.(map[string]interface{})
		if !ok {
			continue
		}
		
		// 2. 提取插件字段
		name, _, _ := unstructured.NestedString(pmap, "name")
		wasmPath, _, _ := unstructured.NestedString(pmap, "wasm_path")
		rawConfig, _, _ := unstructured.NestedMap(pmap, "config") // config 是一个 map[string]string
		
		// 3. 转换 config map (map[string]interface{} -> map[string]string)
		config := make(map[string]string)
		for k, v := range rawConfig {
			// strVal, ok := v.(string);
			if strVal, aa := v.(string); aa {
				config[k] = strVal
			}
		}

		plugins = append(plugins, &agwv1.Plugin{
			Name:     name,
			WasmPath: wasmPath,
			Config:   config,
		})
	}
	return plugins
}

func (c *Controller) onServiceAdd(obj interface{}) {
	// For basic discovery, we rely on EndpointSlices. 
	// Services are useful if we need ClusterIP or NodePort info, or specific ports mapping.
	// For MVP, EndpointSlice contains the Service Name label.
}

func (c *Controller) onServiceUpdate(old, new interface{}) {}

func (c *Controller) onServiceDelete(obj interface{}) {
	svc := obj.(*corev1.Service)
	// Optionally cleanup registry if NO EndpointSlice left? 
	// But usually Slice is deleted too. Let Slice delete handle it?
	// If Service is deleted, Slices are GCed.
	c.registry.DeleteService(svc.Namespace, svc.Name)
}

func (c *Controller) onSliceAdd(obj interface{}) {
	c.processSlice(obj)
}

func (c *Controller) onSliceUpdate(old, new interface{}) {
	c.processSlice(new)
}

func (c *Controller) onSliceDelete(obj interface{}) {
	slice := obj.(*discoveryv1.EndpointSlice)
	svcName := slice.Labels[discoveryv1.LabelServiceName]
	if svcName != "" {
		// For MVP: Treat slice delete as service delete or empty endpoints.
		// Construct minimal cluster with empty endpoints
		cluster := &agwv1.Cluster{
			Name:      fmt.Sprintf("k8s/%s/%s", slice.Namespace, svcName),
			Endpoints: []*agwv1.Endpoint{},
		}
		c.registry.UpdateEndpointSlice(slice, cluster)
	}
}

// processSlice 处理 EndpointSlice 对象的变更，将其转换为网关内部的 Cluster 模型。
// 这是服务发现的核心逻辑：将 K8s 的 "切片" (Slices) 聚合成网关可用的 "集群" (Clusters)。
func (c *Controller) processSlice(obj interface{}) {
	// 1. 类型断言：确保拿到的对象是 EndpointSlice
	slice, ok := obj.(*discoveryv1.EndpointSlice)
	if !ok {
		return
	}
	
	// 2. 获取所属 Service 名称
	// EndpointSlice 通过 Label "kubernetes.io/service-name" 关联到 Service
	svcName := slice.Labels[discoveryv1.LabelServiceName]
	if svcName == "" {
		return
	}

	endpoints := make([]*agwv1.Endpoint, 0)
	
	// 3. 遍历切片中的所有 Endpoint (即 Pod)
	for _, ep := range slice.Endpoints {
		// 3.1 过滤掉未就绪 (Not Ready) 的 Pod
		// 如果 Pod 正在启动或探针失败，不应该转发流量过去
		if ep.Conditions.Ready != nil && !*ep.Conditions.Ready {
			continue
		}
		// 3.2 确保有 IP 地址
		if len(ep.Addresses) == 0 {
			continue
		}
		
		// 3.3 提取端口信息
		// MVP 简化处理：默认取第一个端口，如果没有定义则默认为 80
		var port uint32 = 80
		if len(slice.Ports) > 0 && slice.Ports[0].Port != nil {
			port = uint32(*slice.Ports[0].Port)
		}

		// 3.4 构建内部 Endpoint 对象
		endpoints = append(endpoints, &agwv1.Endpoint{
			Address: ep.Addresses[0], // 通常 Pod 只有一个 IP，取第一个即可
			Port:    port,
		})
	}

	// 4. 构建内部 Cluster 对象
	// 命名规则：k8s/{namespace}/{serviceName}
	// 这样网关的核心逻辑就可以通过这个 ID 找到对应的后端列表
	cluster := &agwv1.Cluster{
		Name:      fmt.Sprintf("k8s/%s/%s", slice.Namespace, svcName),
		Endpoints: endpoints,
	}

	// 5. 更新 Registry
	// 将转换好的 Cluster 数据存入内存，并触发变更通知
	c.registry.UpdateEndpointSlice(slice, cluster)
}

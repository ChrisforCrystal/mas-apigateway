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

type Controller struct {
	client      *kubernetes.Clientset
	dynClient   dynamic.Interface
	factory     informers.SharedInformerFactory
	dynFactory  dynamicinformer.DynamicSharedInformerFactory
	serviceInf  cache.SharedIndexInformer
	sliceInf    cache.SharedIndexInformer
	registry    *Registry
	routeLister cache.GenericLister
	routeSynced cache.InformerSynced
}

func NewController(client *kubernetes.Clientset, dynClient dynamic.Interface, registry *Registry) *Controller {
	factory := informers.NewSharedInformerFactory(client, 30*time.Second) 
	
	serviceInf := factory.Core().V1().Services().Informer()
	sliceInf := factory.Discovery().V1().EndpointSlices().Informer()

	// Dynamic Informer for GatewayRoute
	gvr := schema.GroupVersionResource{
		Group:    "agw.masallsome.io",
		Version:  "v1",
		Resource: "gatewayroutes",
	}
	dynFactory := dynamicinformer.NewDynamicSharedInformerFactory(dynClient, 30*time.Second)
	routeInf := dynFactory.ForResource(gvr).Informer()
	routeLister := dynFactory.ForResource(gvr).Lister()

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

	serviceInf.AddEventHandler(cache.ResourceEventHandlerFuncs{
		AddFunc:    c.onServiceAdd,
		UpdateFunc: c.onServiceUpdate,
		DeleteFunc: c.onServiceDelete,
	})

	sliceInf.AddEventHandler(cache.ResourceEventHandlerFuncs{
		AddFunc:    c.onSliceAdd,
		UpdateFunc: c.onSliceUpdate,
		DeleteFunc: c.onSliceDelete,
	})

	routeInf.AddEventHandler(cache.ResourceEventHandlerFuncs{
		AddFunc:    func(obj interface{}) { c.rebuildRoutes() },
		UpdateFunc: func(old, new interface{}) { c.rebuildRoutes() },
		DeleteFunc: func(obj interface{}) { c.rebuildRoutes() },
	})
	
	// We need to start the dynamic factory inside Run, 
	// so we might need to store it or start it here? 
	// Standard pattern: store factory or just start it in Run.
	// But dynamicinformer.SharedInformerFactory doesn't seem to expose simple Start?
	// Actually it does: Start(stopCh <-chan struct{})
	// Let's store dynFactory in struct if needed, OR just start routeInf directly?
	// dynFactory.Start() starts all created informers.
	// Let's add dynFactory to Controller struct to start it in Run.
	
	// Actually for simplicity, let's start it in Run by creating it there? No, we need lister.
	// Let's modify Controller struct to hold dynFactory in next step if compilation fails, 
	// or assume we can just keep routeInf running? 
	// If we create factory here, we must start it.
	// Let's add `dynFactory dynamicinformer.DynamicSharedInformerFactory` to Controller.
	
	return c
}

func (c *Controller) Run(ctx context.Context) {
	log.Println("Starting K8s Discovery Controller...")
	go c.factory.Start(ctx.Done())
	go c.dynFactory.Start(ctx.Done())
	
	if !cache.WaitForCacheSync(ctx.Done(), c.serviceInf.HasSynced, c.sliceInf.HasSynced, c.routeSynced) {
		log.Println("Timed out waiting for caches to sync")
		return
	}
	gvr := schema.GroupVersionResource{
		Group:    "agw.masallsome.io",
		Version:  "v1",
		Resource: "gatewayroutes",
	}

	// Create Dynamic Informer
	dynInformer := dynamicinformer.NewDynamicSharedInformerFactory(c.dynClient, 0)
	informer := dynInformer.ForResource(gvr).Informer()

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
	dynInformer.Start(ctx.Done())
	cache.WaitForCacheSync(ctx.Done(), informer.HasSynced)
}

func (c *Controller) rebuildRoutes() {
	// List all routes from cache
	objs, err := c.routeLister.List(labels.Everything())
	if err != nil {
		log.Printf("Error listing GatewayRoutes: %v", err)
		return
	}

	var routes []*agwv1.Route
	for _, obj := range objs {
		u, ok := obj.(*unstructured.Unstructured)
		if !ok {
			continue
		}
		
		route := c.parseRoute(u)
		if route != nil {
			routes = append(routes, route)
		}
	}

	log.Printf("Rebuilt %d GatewayRoutes from CRDs", len(routes))
	c.registry.StoreCRDRoutes(routes)
}

func (c *Controller) parseRoute(u *unstructured.Unstructured) *agwv1.Route {
	spec, found, _ := unstructured.NestedMap(u.Object, "spec")
	if !found {
		return nil
	}

	match, _, _ := unstructured.NestedString(spec, "match")
	if match == "" {
		return nil
	}

	backend, _, _ := unstructured.NestedMap(spec, "backend")
	svcName, _, _ := unstructured.NestedString(backend, "service_name")
	
	if svcName == "" {
		return nil // Invalid route
	}

	clusterName := fmt.Sprintf("k8s/%s/%s", u.GetNamespace(), svcName)
	
	plugins := c.parsePlugins(spec)

	return &agwv1.Route{
		PathPrefix: match,
		ClusterId:  clusterName,
		Plugins:    plugins,
	}
}

func (c *Controller) parsePlugins(spec map[string]interface{}) []*agwv1.Plugin {
	rawPlugins, found, _ := unstructured.NestedSlice(spec, "plugins")
	if !found {
		return nil
	}

	var plugins []*agwv1.Plugin
	for _, p := range rawPlugins {
		pmap, ok := p.(map[string]interface{})
		if !ok {
			continue
		}
		
		name, _, _ := unstructured.NestedString(pmap, "name")
		wasmPath, _, _ := unstructured.NestedString(pmap, "wasm_path")
		rawConfig, _, _ := unstructured.NestedMap(pmap, "config")
		
		config := make(map[string]string)
		for k, v := range rawConfig {
			if strVal, ok := v.(string); ok {
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

func (c *Controller) processSlice(obj interface{}) {
	slice, ok := obj.(*discoveryv1.EndpointSlice)
	if !ok {
		return
	}
	svcName := slice.Labels[discoveryv1.LabelServiceName]
	if svcName == "" {
		return
	}

	endpoints := make([]*agwv1.Endpoint, 0)
	for _, ep := range slice.Endpoints {
		if ep.Conditions.Ready != nil && !*ep.Conditions.Ready {
			continue
		}
		if len(ep.Addresses) == 0 {
			continue
		}
		
		var port uint32 = 80
		if len(slice.Ports) > 0 && slice.Ports[0].Port != nil {
			port = uint32(*slice.Ports[0].Port)
		}

		endpoints = append(endpoints, &agwv1.Endpoint{
			Address: ep.Addresses[0],
			Port:    port,
		})
	}

	cluster := &agwv1.Cluster{
		Name:      fmt.Sprintf("k8s/%s/%s", slice.Namespace, svcName),
		Endpoints: endpoints,
	}

	c.registry.UpdateEndpointSlice(slice, cluster)
}

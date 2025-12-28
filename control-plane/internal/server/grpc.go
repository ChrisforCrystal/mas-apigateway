package server

import (
	"fmt"
	"log"
	"sync"
	"time"

	"github.com/masallsome/masapigateway/control-plane/pkg/config"
	"github.com/masallsome/masapigateway/control-plane/pkg/k8s"
	agwv1 "github.com/masallsome/masapigateway/control-plane/pkg/proto"
	"google.golang.org/grpc"
)

type AgwServer struct {
	// 继承 UnimplementedAgwServiceServer 以保证向前兼容性
	agwv1.UnimplementedAgwServiceServer
	
	watcher      *config.Watcher // 监听本地静态配置文件
	registry     *k8s.Registry   // 监听 K8s 动态资源 (CRD, Secret)
	
	mu           sync.RWMutex    // 读写锁，保护下面的 clients 映射表
	
	// clients 维护了所有当前连接的数据平面 (Data Plane) 实例。
	// Key: int64 (nextID 生成的唯一连接 ID)
	// Value: chan *agwv1.ConfigSnapshot (发送配置快照的管道)
	//
	// 【为什么要用 chan?】
	// 1. **解耦发送与处理**：控制平面生成新配置后，只需往管道里“丢”一份快照即可，不需要等待网络发送完成。
	// 2. **异步广播**：当配置变更时，我们可以遍历所有 clients，通过 channel 并发地把新配置推给每一个连接，而不会因为某个连接网络卡顿而阻塞整个控制平面的更新流程。
	// 3. **作为缓冲区**：如果数据平面处理慢，channel 可以起到微小的缓冲作用（虽然这里大多是一次性推送）。
	clients      map[int64]chan *agwv1.ConfigSnapshot
	
	nextID       int64                  // 用于生成下一个 client 的唯一 ID
	current      *agwv1.ConfigSnapshot  // 当前最新的、已合并的全局配置快照 (缓存)
	staticConfig *agwv1.ConfigSnapshot  // 从本地文件加载的静态配置 (作为基底)
}

func NewAgwServer(watcher *config.Watcher, registry *k8s.Registry) *AgwServer {
	s := &AgwServer{
		watcher:  watcher,
		registry: registry,
		clients:  make(map[int64]chan *agwv1.ConfigSnapshot),
	}
	// Start loop
	go s.runLoop()
	return s
}

func (s *AgwServer) runLoop() {
	go func() {
		if err := s.watcher.Start(); err != nil {
			log.Printf("Watcher failed: %v", err)
		}
	}()
	
	// Initial empty static config to avoid nil
	// 初始化静态配置快照，默认为空，等待第一次加载
	s.staticConfig = &agwv1.ConfigSnapshot{VersionId: "init"}

	// 获取两个关键的事件通知通道：
	// 1. registryCh: 监听 K8s 动态资源 (CRD, Secret, Service) 的变更信号
	// 2. watcherCh:  监听本地静态配置文件 (config.yaml) 的内容变更
	var registryCh <-chan struct{}
	if s.registry != nil {
		registryCh = s.registry.Updates()
	}
	watcherCh := s.watcher.Updates()
	
	// 启动【控制平面主事件循环】(Main Event Loop)
	// 这里的 select 类似于多路复用器，同时等待来自两个方向的变更通知。
	for {
		select {
		// 情况 A: 本地静态配置文件变了
		case snapshot, ok := <-watcherCh:
			if !ok {
				log.Println("Watcher channel closed, stopping runLoop")
				return // 通道关闭，退出循环 (通常是程序关闭时)
			}
			// 更新内存中的静态配置基底
			s.staticConfig = snapshot
			// 触发合并广播：静态配置 + 动态 K8s 配置
			s.broadcastMerged()

		// 情况 B: K8s 里的资源变了 (Registry 发出了信号)
		case _, ok := <-registryCh:
			// 注意：如果 registryCh 为 nil (即 K8s 未启用)，select 会永远忽略这个 case，这是安全的。
			if !ok {
				log.Println("Registry channel closed")
				return
			}
			// 触发合并广播
			s.broadcastMerged()
		}
	}
}

// broadcastMerged 将 "静态配置" 和 "动态 K8s 配置" 合并成一份最终配置，
// 然后推送给所有连接的数据平面客户端。
func (s *AgwServer) broadcastMerged() {
	// 加锁，确保在生成快照的过程中，不会有新的客户端连接进来干扰，保证线程安全
	s.mu.Lock()
	defer s.mu.Unlock()
	
	// 1. 获取本地静态配置的基底
	staticCfg := s.staticConfig 

	// 准备 K8s 数据 (如果 Registry 存在)
	var k8sRoutes []*agwv1.Route
	var k8sClusters []*agwv1.Cluster
	if s.registry != nil {
		k8sRoutes = s.registry.ListRoutes()
		k8sClusters = s.registry.ListClusters()
	}

	// 2. 创建一个新的配置快照对象 (Snapshot)，开始【合并】逻辑
	snapshot := &agwv1.ConfigSnapshot{
		Listeners: staticCfg.Listeners, // 暂时先引用静态 Listeners (后面会处理 TLS 证书注入)
		// 【合并路由】：将静态文件的 Routes 和 K8s Registry 里的 CRD Routes 拼接到一起
		// append(A, B...) 语法将 B 切片打散追加到 A 后面
		Routes: append(staticCfg.Routes, k8sRoutes...),
		// 【合并集群】：先放入静态集群 (通常为空或测试用)
		Clusters: staticCfg.Clusters,
		// 【合并资源】：Redis 和数据库配置 (直接引用静态配置，因为目前 K8s 侧没有对应 CRD)
		Resources: staticCfg.Resources,
	}

	// 继续追加 K8s 中发现的服务集群 (EndpointSlices 转换而来)
	snapshot.Clusters = append(snapshot.Clusters, k8sClusters...)
	
	// 3. 【注入 TLS 证书】 (Resolve Secrets)
	// 这一步非常关键：因为 Proto 定义里的 SecretName 只是一个字符串引用，
	// 数据面 Data Plane 需要真正的证书内容 (PEM 格式) 才能启动 HTTPS。
	// 我们需要遍历所有 Listener，如果发现它引用了 Secret，就去 Registry 里把 Secret 内容挖出来填进去。
	
	// 创建一个新的 Listener 切片，容量与静态配置一致
	newListeners := make([]*agwv1.Listener, 0, len(staticCfg.Listeners))
	for _, l := range staticCfg.Listeners {
		// 浅拷贝 (Shallow Copy) Listener 结构体本身
		// 为什么？因为我们即将修改里面的 Tls 字段。如果不拷贝直接改，会污染 s.staticConfig 原本的数据，
		// 导致下次合并时逻辑出错。
		nl := *l
		
		// 如果该监听器开启了 TLS 并且指定了 Secret 名字
		if nl.Tls != nil && nl.Tls.SecretName != "" {
			// 去 Registry 查找这是不是一个已经缓存的 K8s Secret
			// 只有当 Registry 启用时才去查找
			var secret *k8s.TlsSecret
			if s.registry != nil {
				secret = s.registry.GetSecret(nl.Tls.SecretName)
			}

			if secret != nil {
				// 同样，我们需要深拷贝 TlsConfig，避免修改原始指针指向的对象
				newTls := *nl.Tls
				// 【核心动作】：把 K8s Secret 里存的证书内容 (Cert/Key) 填充到配置对象里
				newTls.CertPem = secret.Cert
				newTls.KeyPem = secret.Key
				nl.Tls = &newTls // 指向新的包含了证书内容的 TlsConfig
			} else {
				log.Printf("Warning: Secret %s not found for listener %s (Registry capable: %v)", nl.Tls.SecretName, nl.Name, s.registry != nil)
			}
		}
		// 将处理好的（可能注入了证书的）Listener 加入新列表
		newListeners = append(newListeners, &nl)
	}
	// 用处理好的 Listener 列表替换快照里的旧列表
	snapshot.Listeners = newListeners

	// 4. 生成新版本号
	// 格式：静态版本-k8s-当前时间戳。这样数据面可以知道配置是否更新。
	version := fmt.Sprintf("%s-k8s-%s", staticCfg.VersionId, time.Now().Format("150405"))
	snapshot.VersionId = version 

	// 更新服务器持有的最新快照
	s.current = snapshot 

	// 5. 【广播推送】 (Broadcasting)
	if len(s.clients) > 0 {
		log.Printf("Broadcasting merged config version %s (Static Routes: %d, CRD Routes: %d, Static Clusters: %d, K8s Clusters: %d)",
			version, len(staticCfg.Routes), len(k8sRoutes), len(staticCfg.Clusters), len(k8sClusters))

		// 遍历所有已连接的数据面客户端
		for _, ch := range s.clients { 
			// 使用 select + default 进行非阻塞发送
			// 如果某个客户端处理太慢导致 channel 满了，我们选择跳过它而不是阻塞整个控制平面
			// (生产环境可能需要更复杂的重试或断开重连机制)
			select {
			case ch <- snapshot:
				// 发送成功
			default:
				log.Println("Warning: client channel full, skipping update")
			}
		}
	}
}

// registerClient 将一个新的数据平面连接注册到 clients 映射表中。
// 返回生成的 clientID，以便后续注销。
func (s *AgwServer) registerClient(ch chan *agwv1.ConfigSnapshot) int64 {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	id := s.nextID
	s.nextID++
	s.clients[id] = ch // 把这个连接的专属信箱放入总列表
	
	// 如果此时已经有配置了，立刻发送一份当前的最新配置给新来的客户端
	// 这样新启动的 Data Plane 不用等到下一次配置变更就能拿到初始配置
	if s.current != nil {
		go func() { ch <- s.current }()
	}
	return id
}

// unregisterClient 当连接断开时，从列表中移除该客户端。
func (s *AgwServer) unregisterClient(id int64) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.clients, id)
}

// StreamConfig 是 gRPC 接口的具体实现。
// 每一个连接上来的 Data Plane 都会触发一个新的 StreamConfig Goroutine。
func (s *AgwServer) StreamConfig(req *agwv1.Node, stream grpc.ServerStreamingServer[agwv1.ConfigSnapshot]) error {
	log.Printf("New node connected: ID=%s Region=%s Version=%s", req.Id, req.Region, req.Version)

	// 1. 创建一个专属的通道 (信箱)
	// 这个通道用来接收来自 broadcastMerged 的配置快照
	updateChan := make(chan *agwv1.ConfigSnapshot, 1)
	
	// 2. 注册：把这封信箱交给 AgwServer 管理
	id := s.registerClient(updateChan)
	// 3. 确保退出时注销 (defer)
	defer s.unregisterClient(id)

	// 4. 进入死循环，守着这两个来源：
	for {
		select {
		// A: 收到新配置了！(来自 updateChan)
		// 这里的 snapshot 就是 broadcastMerged里 `case ch <- snapshot` 塞进来的那个
		case snapshot := <-updateChan:
			// 执行真正的网络发送
			if err := stream.Send(snapshot); err != nil {
				log.Printf("Error sending to %s: %v", req.Id, err)
				return err // 发送失败（比如网络断了），函数返回，连接断开
			}
		
		// B: 客户端主动断开了连接
		case <-stream.Context().Done():
			return nil
		}
	}
}

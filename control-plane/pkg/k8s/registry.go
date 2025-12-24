package k8s

import (
	"fmt"
	"sync"

	agwv1 "github.com/masallsome/masapigateway/control-plane/pkg/proto"
	discoveryv1 "k8s.io/api/discovery/v1"
)

// Registry 保存了已发现的 K8s 服务的当前状态。
// 它将 Service 键 (namespace/name) 映射到 Cluster 快照。
// Registry 用于维护从 Kubernetes 集群中同步过来的服务、路由和密钥信息，
// 并提供给控制平面主循环使用，以便生成最新的配置推送给数据平面。
type Registry struct {
	mu       sync.RWMutex
	clusters map[string]*agwv1.Cluster // 存储服务集群信息，key 为 "namespace/serviceName"
	routes   []*agwv1.Route            // 存储从 CRD 或 Ingress 转换而来的路由规则
	secrets  map[string]*TlsSecret     // 存储 TLS 证书和密钥，key 为 Secret 名称
	updates  chan struct{}             // 信号通道，用于通知 Registry 状态发生变化
}

// TlsSecret 封装了 TLS 证书和私钥的字节内容。
type TlsSecret struct {
	Cert []byte
	Key  []byte
}

// NewRegistry 创建并初始化一个新的 Registry 实例。
func NewRegistry() *Registry {
	return &Registry{
		clusters: make(map[string]*agwv1.Cluster),
		routes:   make([]*agwv1.Route, 0),
		secrets:  make(map[string]*TlsSecret),
		updates:  make(chan struct{}, 1),
	}
}

// Updates 返回一个通道，当 Registry 发生变化时会收到信号。
// 它使用带 1 个缓冲区的非阻塞发送，起到 "脏位 (dirty bit)" 信号的作用。
// 调用者可以通过监听此通道来获知需要重新生成和推送配置的时机。
func (r *Registry) Updates() <-chan struct{} {
	return r.updates
}

// notify 向 updates 通道发送信号，通知观察者 Registry 已更新。
// 如果通道已满（已有待处理信号），则丢弃本次信号，避免阻塞。
func (r *Registry) notify() {
	select {
	case r.updates <- struct{}{}:
	default:
	}
}

// UpdateEndpointSlice 处理 EndpointSlice 并更新相应的 Cluster 信息。
// 对于 MVP 版本，我们假设 1 个 Service 对应 1 个 Cluster。
// 命名约定: "k8s/{namespace}/{service_name}"
func (r *Registry) UpdateEndpointSlice(slice *discoveryv1.EndpointSlice, cluster *agwv1.Cluster) {
	r.mu.Lock()
	defer r.mu.Unlock()
	
	key := fmt.Sprintf("%s/%s", slice.Namespace, slice.Labels["kubernetes.io/service-name"])
	// 在真实的实现中，一个 Service 可能对应多个 EndpointSlice，我们需要合并它们。
	// 对于 MVP，我们简化处理，直接基于服务名覆盖/更新。
	// 理想情况下，应该映射 Slice -> Endpoints 并进行聚合。
	
	r.clusters[key] = cluster
	r.notify()
}

// DeleteService 从 Registry 中删除指定的服务。
func (r *Registry) DeleteService(namespace, name string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	
	key := fmt.Sprintf("%s/%s", namespace, name)
	delete(r.clusters, key)
	r.notify()
}

// ListClusters 返回所有已发现的 Cluster 列表。
// 返回的是 Cluster 指针的切片。
func (r *Registry) ListClusters() []*agwv1.Cluster {
	r.mu.RLock()
	defer r.mu.RUnlock()
	list := make([]*agwv1.Cluster, 0, len(r.clusters))
	for _, c := range r.clusters {
		list = append(list, c)
	}
	return list
}

// StoreCRDRoutes 更新 Registry 中的路由规则。
// 这些路由通常来自自定义资源 (CRD) 或 Ingress 资源的转换结果。
func (r *Registry) StoreCRDRoutes(routes []*agwv1.Route) {
	// 获取写锁 (Write Lock)：互斥锁，确保同一时间只有一个协程能修改路由表
	// 在持有写锁期间，任何其他协程的读锁 (RLock) 和写锁 (Lock) 请求都会被阻塞
	r.mu.Lock()
	defer r.mu.Unlock() // 函数退出时自动释放锁
	
	// 全量替换路由列表
	r.routes = routes
	
	// 触发变更通知，告知控制平面主循环配置已更新
	r.notify()
}

// ListRoutes 返回当前存储的所有路由规则。
// 为了并发安全，返回的是路由切片的副本。
func (r *Registry) ListRoutes() []*agwv1.Route {
	r.mu.RLock()
	defer r.mu.RUnlock()
	// Return a copy slice
	list := make([]*agwv1.Route, len(r.routes))
	copy(list, r.routes)
	return list
}

// UpdateSecret 更新或添加一个 TLS Secret。
func (r *Registry) UpdateSecret(name string, cert, key []byte) {
	r.mu.Lock()
	defer r.mu.Unlock()
	
	r.secrets[name] = &TlsSecret{
		Cert: cert,
		Key:  key,
	}
	r.notify()
}

// DeleteSecret 从 Registry 中删除指定的 TLS Secret。
func (r *Registry) DeleteSecret(name string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	
	delete(r.secrets, name)
	r.notify()
}

// GetSecret 根据名称获取 TLS Secret。
// 如果未找到，返回 nil。
func (r *Registry) GetSecret(name string) *TlsSecret {
	r.mu.RLock()
	defer r.mu.RUnlock()
	
	return r.secrets[name] // returns nil if not found
}

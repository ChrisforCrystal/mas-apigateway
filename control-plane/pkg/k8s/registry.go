package k8s

import (
	"fmt"
	"sync"

	agwv1 "github.com/masallsome/masapigateway/control-plane/pkg/proto"
	discoveryv1 "k8s.io/api/discovery/v1"
)

// Registry holds the current state of discovered K8s services.
// It maps Service keys (namespace/name) to Cluster snapshots.
type Registry struct {
	mu       sync.RWMutex
	clusters map[string]*agwv1.Cluster
	routes   []*agwv1.Route // Routes from CRD/Ingress
	secrets  map[string]*TlsSecret
	updates  chan struct{}  // Signal channel
}

type TlsSecret struct {
	Cert []byte
	Key  []byte
}

func NewRegistry() *Registry {
	return &Registry{
		clusters: make(map[string]*agwv1.Cluster),
		routes:   make([]*agwv1.Route, 0),
		secrets:  make(map[string]*TlsSecret),
		updates:  make(chan struct{}, 1),
	}
}

// Updates returns a channel that signals when registry changes.
// It uses a non-blocking send with 1-buffer to function as a "dirty" signal.
func (r *Registry) Updates() <-chan struct{} {
	return r.updates
}

func (r *Registry) notify() {
	select {
	case r.updates <- struct{}{}:
	default:
	}
}

// UpdateEndpointSlice processes an EndpointSlice and updates the corresponding Cluster.
// For MVP, we assume 1 Service = 1 Cluster.
// naming convention: "k8s/{namespace}/{service_name}"
func (r *Registry) UpdateEndpointSlice(slice *discoveryv1.EndpointSlice, cluster *agwv1.Cluster) {
	r.mu.Lock()
	defer r.mu.Unlock()
	
	key := fmt.Sprintf("%s/%s", slice.Namespace, slice.Labels["kubernetes.io/service-name"])
	// In a real implementation with multiple slices per service, we need to merge them.
	// For MVP, we just overwrite/upsert based on service name (simplified).
	// Ideally we map Slice -> Endpoints and aggregate.
	
	r.clusters[key] = cluster
	r.notify()
}

func (r *Registry) DeleteService(namespace, name string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	
	key := fmt.Sprintf("%s/%s", namespace, name)
	delete(r.clusters, key)
	r.notify()
}

// ListClusters returns all discovered clusters.
func (r *Registry) ListClusters() []*agwv1.Cluster {
	r.mu.RLock()
	defer r.mu.RUnlock()
	list := make([]*agwv1.Cluster, 0, len(r.clusters))
	for _, c := range r.clusters {
		list = append(list, c)
	}
	return list
}

func (r *Registry) StoreCRDRoutes(routes []*agwv1.Route) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.routes = routes
	r.notify()
}

func (r *Registry) ListRoutes() []*agwv1.Route {
	r.mu.RLock()
	defer r.mu.RUnlock()
	// Return a copy slice
	list := make([]*agwv1.Route, len(r.routes))
	copy(list, r.routes)
	return list
}

func (r *Registry) UpdateSecret(name string, cert, key []byte) {
	r.mu.Lock()
	defer r.mu.Unlock()
	
	r.secrets[name] = &TlsSecret{
		Cert: cert,
		Key:  key,
	}
	r.notify()
}

func (r *Registry) DeleteSecret(name string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	
	delete(r.secrets, name)
	r.notify()
}

func (r *Registry) GetSecret(name string) *TlsSecret {
	r.mu.RLock()
	defer r.mu.RUnlock()
	
	return r.secrets[name] // returns nil if not found
}

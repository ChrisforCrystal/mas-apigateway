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
	agwv1.UnimplementedAgwServiceServer
	watcher      *config.Watcher
	registry     *k8s.Registry
	mu           sync.RWMutex
	clients      map[int64]chan *agwv1.ConfigSnapshot
	nextID       int64
	current      *agwv1.ConfigSnapshot
	staticConfig *agwv1.ConfigSnapshot
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
	s.staticConfig = &agwv1.ConfigSnapshot{VersionId: "init"}

	// We need a loop that selects on both
	// watcher.Updates() -> update s.staticConfig -> Merge & Broadcast
	// registry.Updates() -> Merge & Broadcast
	
	// Create a ticker to force sync? No, registry has signal.
	
	registryCh := s.registry.Updates()
	watcherCh := s.watcher.Updates()
	
	for {
		select {
		case snapshot, ok := <-watcherCh:
			if !ok {
				return // Watcher closed
			}
			s.staticConfig = snapshot
			s.broadcastMerged()
		case <-registryCh:
			s.broadcastMerged()
		}
	}
}

func (s *AgwServer) broadcastMerged() {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	// Start with Static Config Copy
	// Deep copy to avoid mutating the original config snapshot
	// For MVP, we reconstruct the snapshot
	staticCfg := s.staticConfig // Use a local variable for clarity

	// Create a new snapshot based on static + dynamic
	snapshot := &agwv1.ConfigSnapshot{
		Listeners: staticCfg.Listeners, // Static Listeners
		// Merged Routes (Static + CRD)
		Routes: append(staticCfg.Routes, s.registry.ListRoutes()...),
		// Merged Clusters (Static + K8s)
		Clusters: staticCfg.Clusters,
	}

	// Merge K8s Clusters
	k8sClusters := s.registry.ListClusters()
	snapshot.Clusters = append(snapshot.Clusters, k8sClusters...)
	
	// Resolve Secrets (TLS)
	// We need to iterate over Listeners and check if they have SecretName, then fill Cert/Key
	// Since we are iterating pointers in a slice that might be shared (staticCfg.Listeners), 
	// we should clone the listeners if we are modifying them, or we assume loader.go created fresh ones for each snapshot?
	// The staticCfg is replaced on file change, but safe to be careful.
	// Actually, staticCfg is just a pointer.
	// Let's create a new Listeners slice for the snapshot.
	
	newListeners := make([]*agwv1.Listener, 0, len(staticCfg.Listeners))
	for _, l := range staticCfg.Listeners {
		// Shallow copy listener struct
		nl := *l
		// If TLS config exists, handle it
		if nl.Tls != nil && nl.Tls.SecretName != "" {
			// Look up secret
			if secret := s.registry.GetSecret(nl.Tls.SecretName); secret != nil {
				// We need to duplicate TlsConfig to avoid modifying the static one in place if it's reused?
				// Yes.
				newTls := *nl.Tls
				newTls.CertPem = secret.Cert
				newTls.KeyPem = secret.Key
				nl.Tls = &newTls
			} else {
				log.Printf("Warning: Secret %s not found for listener %s", nl.Tls.SecretName, nl.Name)
			}
		}
		newListeners = append(newListeners, &nl)
	}
	snapshot.Listeners = newListeners

	// Versioning
	version := fmt.Sprintf("%s-k8s-%s", staticCfg.VersionId, time.Now().Format("150405"))
	snapshot.VersionId = version // Assign to VersionId

	s.current = snapshot // Update the current snapshot

	if len(s.clients) > 0 {
		log.Printf("Broadcasting merged config version %s (Static Routes: %d, CRD Routes: %d, Static Clusters: %d, K8s Clusters: %d)",
			version, len(staticCfg.Routes), len(s.registry.ListRoutes()), len(staticCfg.Clusters), len(k8sClusters))

		for _, ch := range s.clients { // Send to all subscribers using a separate goroutine or buffered channel to avoid blocking?
			// For MVP, blocking send with select
			select {
			case ch <- snapshot:
			default:
				log.Println("Warning: client channel full, skipping update")
			}
		}
	}
}

func (s *AgwServer) registerClient(ch chan *agwv1.ConfigSnapshot) int64 {
	s.mu.Lock()
	defer s.mu.Unlock()
	id := s.nextID
	s.nextID++
	s.clients[id] = ch
	
	if s.current != nil {
		go func() { ch <- s.current }()
	}
	return id
}

func (s *AgwServer) unregisterClient(id int64) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.clients, id)
}

func (s *AgwServer) StreamConfig(req *agwv1.Node, stream grpc.ServerStreamingServer[agwv1.ConfigSnapshot]) error {
	log.Printf("New node connected: ID=%s Region=%s Version=%s", req.Id, req.Region, req.Version)

	updateChan := make(chan *agwv1.ConfigSnapshot, 1)
	id := s.registerClient(updateChan)
	defer s.unregisterClient(id)

	for {
		select {
		case snapshot := <-updateChan:
			if err := stream.Send(snapshot); err != nil {
				log.Printf("Error sending to %s: %v", req.Id, err)
				return err
			}
		case <-stream.Context().Done():
			return nil
		}
	}
}

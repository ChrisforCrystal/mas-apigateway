package main

import (
	"context"
	"log"
	"net"
	"os"

	"github.com/masallsome/masapigateway/control-plane/internal/server"
	serverConfig "github.com/masallsome/masapigateway/control-plane/pkg/config"
	"github.com/masallsome/masapigateway/control-plane/pkg/k8s"
	agwv1 "github.com/masallsome/masapigateway/control-plane/pkg/proto"
	"google.golang.org/grpc"
)

func main() {
	port := os.Getenv("PORT")
	if port == "" {
		port = "18000"
	}
	
	lis, err := net.Listen("tcp", ":"+port)
	if err != nil {
		log.Fatalf("failed to listen: %v", err)
	}
	
	// Initialize Config Watcher
	configPath := os.Getenv("AGW_CONFIG_PATH")
	if configPath == "" {
		configPath = "config.yaml"
	}
	watcher, err := serverConfig.NewWatcher(configPath)
	if err != nil {
		log.Printf("Warning: failed to create watcher: %v", err)
	}

	// 1. Initialize K8s Client
	ctx := context.Background()
	clientset, _, err := k8s.NewClient()
	if err != nil {
		log.Printf("Warning: failed to create K8s client: %v (K8s Discovery Disabled)", err)
	}
	
	dynClient, err := k8s.NewDynamicClient()
	if err != nil {
		log.Printf("Warning: failed to create Dynamic client: %v", err)
	}

	var k8sRegistry *k8s.Registry
	if clientset != nil && dynClient != nil {
		k8sRegistry = k8s.NewRegistry()
		// Start K8s Controller
		go func() {
			log.Println("Starting K8s Discovery Controller...")
			ctrl := k8s.NewController(clientset, dynClient, k8sRegistry)
			ctrl.Run(ctx)
		}()
		
		// Start Secret Controller
		go func() {
			log.Println("Starting Secret Controller...")
			ctrl := k8s.NewSecretController(clientset, k8sRegistry)
			ctrl.Run(ctx)
		}()
	}

	s := grpc.NewServer()
	agwv1.RegisterAgwServiceServer(s, server.NewAgwServer(watcher, k8sRegistry))
	
	log.Printf("Control Plane listening on port %s", port)
	if err := s.Serve(lis); err != nil {
		log.Fatalf("failed to serve: %v", err)
	}
}

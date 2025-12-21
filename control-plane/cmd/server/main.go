package main

import (
	"log"
	"net"
	"os"

	"github.com/masallsome/masapigateway/control-plane/internal/server"
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
	
	s := grpc.NewServer()
	agwv1.RegisterAgwServiceServer(s, server.NewAgwServer())
	
	log.Printf("Control Plane listening on port %s", port)
	if err := s.Serve(lis); err != nil {
		log.Fatalf("failed to serve: %v", err)
	}
}

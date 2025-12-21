package server

import (
	"log"

	agwv1 "github.com/masallsome/masapigateway/control-plane/pkg/proto"
	"google.golang.org/grpc"
)

type AgwServer struct {
	agwv1.UnimplementedAgwServiceServer
}

func (s *AgwServer) StreamConfig(req *agwv1.Node, stream grpc.ServerStreamingServer[agwv1.ConfigSnapshot]) error {
	log.Printf("New node connected: ID=%s Region=%s Version=%s", req.Id, req.Region, req.Version)

	// Send initial snapshot
	initialSnapshot := &agwv1.ConfigSnapshot{
		VersionId: "v1-initial",
	}
	
	if err := stream.Send(initialSnapshot); err != nil {
		return err
	}
	
	log.Println("Sent initial config snapshot")
	
	// Keep stream open (in real world, we'd wait for updates)
	// For MVP, just block forever or return
	select {}
}

func NewAgwServer() *AgwServer {
	return &AgwServer{}
}

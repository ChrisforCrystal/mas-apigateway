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

// main 是 Control Plane 服务的入口函数
// 它负责初始化所有必要的组件，包括：
// 1. 网络监听器
// 2. 本地配置文件监听器
// 3. Kubernetes 客户端和控制器（用于服务发现和 Secret 管理）
// 4. gRPC 服务器，用于处理 Data Plane 的请求
func main() {
	// ==========================================
	// 1. 获取并配置服务端口
	// ==========================================
	// 从环境变量获取 PORT，如果未设置则默认为 18000
	// 这个端口用于 gRPC 服务监听
	port := os.Getenv("PORT")
	if port == "" {
		port = "18000"
	}
	
	// ==========================================
	// 2. 启动网络监听
	// ==========================================
	// 在指定端口启动 TCP 监听器
	lis, err := net.Listen("tcp", ":"+port)
	if err != nil {
		log.Fatalf("failed to listen: %v", err)
	}
	
	// ==========================================
	// 3. 初始化配置监听器 (Watcher)
	// ==========================================
	// 监听本地配置文件（默认为 config.yaml）的变化
	// 当文件发生变化时，Watcher 会通知订阅者，实现配置的热加载
	// 可以通过环境变量 AGW_CONFIG_PATH 自定义配置文件路径
	configPath := os.Getenv("AGW_CONFIG_PATH")
	if configPath == "" {
		configPath = "config.yaml"
	}
	watcher, err := serverConfig.NewWatcher(configPath)
	if err != nil {
		// Watcher 初始化失败只打印警告，不中断程序，可能运行在无配置文件的模式下
		log.Printf("Warning: failed to create watcher: %v", err)
	}

	// ==========================================
	// 4. 初始化 Kubernetes 客户端
	// ==========================================
	ctx := context.Background()
	
	// 初始化标准 K8s 客户端 (Clientset)
	// 用于访问标准的 K8s 资源，如 Services, Secrets, Pods 等
	clientset, _, err := k8s.NewClient()
	if err != nil {
		log.Printf("Warning: failed to create K8s client: %v (K8s Discovery Disabled)", err)
	}
	
	// 初始化动态 K8s 客户端 (DynamicClient)
	// 用于访问自定义资源 (CRDs) 或在不知道具体类型的情况下访问资源
	dynClient, err := k8s.NewDynamicClient()
	if err != nil {
		log.Printf("Warning: failed to create Dynamic client: %v", err)
	}

	// ==========================================
	// 5. 初始化并启动 Kubernetes 控制器
	// ==========================================
	var k8sRegistry *k8s.Registry
	// 只有当 K8s 客户端都成功初始化后，才启动 K8s 相关的功能
	if clientset != nil && dynClient != nil {
		// 初始化 K8s 注册表 (Registry)
		// Registry 用于在其内存中存储 K8s 集群中发现的服务和配置信息
		// Data Plane 可以通过 gRPC 接口查询这些信息
		k8sRegistry = k8s.NewRegistry()
		
		// 启动 K8s 服务发现控制器 (Discovery Controller)
		// 负责监听 K8s Service, EndpointSlice, Ingress 等资源的变化
		// 并将最新的服务拓扑信息同步到 Registry 中
		go func() {
			log.Println("Starting K8s Discovery Controller...")
			ctrl := k8s.NewController(clientset, dynClient, k8sRegistry)
			ctrl.Run(ctx)
		}()
		
		// 启动 Secret 控制器 (Secret Controller)
		// 负责监听 K8s Secret 资源的变化（特别是 TLS 证书）
		// 并将证书数据同步到 Registry 中，供 Data Plane 拉取用于 HTTPS 终结
		go func() {
			log.Println("Starting Secret Controller...")
			ctrl := k8s.NewSecretController(clientset, k8sRegistry)
			ctrl.Run(ctx)
		}()
	}

	// ==========================================
	// 6. 初始化 gRPC 服务器
	// ==========================================
	// 创建 gRPC 服务器实例
	s := grpc.NewServer()
	
	// 注册 AgwService 服务
	// server.NewAgwServer 创建具体的服务实现，传入 watcher 和 k8sRegistry
	// 这样服务实现就能获取到最新的配置和 K8s 集群信息
	agwv1.RegisterAgwServiceServer(s, server.NewAgwServer(watcher, k8sRegistry))
	
	// ==========================================
	// 7. 启动服务
	// ==========================================
	log.Printf("Control Plane listening on port %s", port)
	// 开始处理请求，这是一个阻塞调用
	if err := s.Serve(lis); err != nil {
		log.Fatalf("failed to serve: %v", err)
	}
}

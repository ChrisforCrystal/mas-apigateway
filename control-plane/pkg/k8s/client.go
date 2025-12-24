package k8s

import (
	"fmt"
	"os"
	"path/filepath"

	"k8s.io/client-go/dynamic"
	"k8s.io/client-go/kubernetes"
	"k8s.io/client-go/rest"
	"k8s.io/client-go/tools/clientcmd"
	"k8s.io/client-go/util/homedir"
)

// NewClient 创建并返回一个新的 Kubernetes Clientset。
// 它负责初始化与 Kubernetes API Server 交互的标准客户端。
// 配置加载策略遵循 getRestConfig 中的定义：环境变量 -> 本地配置 -> 集群内部配置。
func NewClient() (*kubernetes.Clientset, *rest.Config, error) {
	// 获取 Kubernetes REST 配置
	config, err := getRestConfig()
	if err != nil {
		return nil, nil, err
	}

	// 使用配置创建 Clientset
	clientset, err := kubernetes.NewForConfig(config)
	if err != nil {
		return nil, nil, fmt.Errorf("failed to create k8s client: %w", err) // 创建失败，返回包装后的错误
	}

	return clientset, config, nil
}

// CRD的
// NewDynamicClient 创建并返回一个新的 Dynamic Client。
// Dynamic Client 用于处理未知的或自定义的 Kubernetes 资源（如 CRD）。
func NewDynamicClient() (*dynamic.DynamicClient, error) {
	// 获取 Kubernetes REST 配置
	config, err := getRestConfig()
	if err != nil {
		return nil, err
	}

	// 使用配置创建 Dynamic Client
	client, err := dynamic.NewForConfig(config)
	if err != nil {
		return nil, fmt.Errorf("failed to create dynamic client: %w", err) // 创建失败，返回包装后的错误
	}

	return client, nil
}

// getRestConfig 尝试通过多种策略获取 Kubernetes REST 配置。
// 优先级顺序：
// 1. KUBECONFIG 环境变量（通常用于开发环境指定特定配置）
// 2. ~/.kube/config 文件（本地开发环境的默认路径）
// 3. In-Cluster Config（生产环境，Pod 内部自动加载 ServiceAccount Token）
func getRestConfig() (*rest.Config, error) {
	// 1. 尝试从 KUBECONFIG 环境变量加载配置 (开发模式常用)
	// 如果设置了 KUBECONFIG 环境变量，则直接使用该路径下的配置文件
	if kubeConfigPath := os.Getenv("KUBECONFIG"); kubeConfigPath != "" {
		return clientcmd.BuildConfigFromFlags("", kubeConfigPath)
	}

	// 2. 尝试加载默认的 ~/.kube/config 文件 (本地默认)
	// 如果没有设置环境变量，尝试查找用户主目录下的 .kube/config 文件
	if home := homedir.HomeDir(); home != "" {
		configPath := filepath.Join(home, ".kube", "config")
		// 检查文件是否存在
		if _, err := os.Stat(configPath); err == nil {
			return clientcmd.BuildConfigFromFlags("", configPath)
		}
	}

	// 3. 回退到 In-Cluster Config (生产环境)
	// 如果以上两种方式都失败，假设程序运行在 Kubernetes Pod 中，使用 ServiceAccount 进行认证
	return rest.InClusterConfig()
}

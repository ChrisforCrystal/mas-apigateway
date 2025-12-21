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

// NewClient returns a new Kubernetes clientset.
// It explicitly parses the KUBECONFIG environment variable or defaults to In-Cluster config.
func NewClient() (*kubernetes.Clientset, *rest.Config, error) {
	config, err := getRestConfig()
	if err != nil {
		return nil, nil, err
	}

	clientset, err := kubernetes.NewForConfig(config)
	if err != nil {
		return nil, nil, fmt.Errorf("failed to create k8s client: %w", err)
	}

	return clientset, config, nil
}

// NewDynamicClient returns a new Dynamic Kubernetes client.
func NewDynamicClient() (*dynamic.DynamicClient, error) {
	config, err := getRestConfig()
	if err != nil {
		return nil, err
	}

	client, err := dynamic.NewForConfig(config)
	if err != nil {
		return nil, fmt.Errorf("failed to create dynamic client: %w", err)
	}

	return client, nil
}

func getRestConfig() (*rest.Config, error) {
	// 1. Try KUBECONFIG env (dev mode)
	if kubeConfigPath := os.Getenv("KUBECONFIG"); kubeConfigPath != "" {
		return clientcmd.BuildConfigFromFlags("", kubeConfigPath)
	}

	// 2. Try ~/.kube/config (local default)
	if home := homedir.HomeDir(); home != "" {
		configPath := filepath.Join(home, ".kube", "config")
		if _, err := os.Stat(configPath); err == nil {
			return clientcmd.BuildConfigFromFlags("", configPath)
		}
	}

	// 3. Fallback to In-Cluster Config (production)
	return rest.InClusterConfig()
}

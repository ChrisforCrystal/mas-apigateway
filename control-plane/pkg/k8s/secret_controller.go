package k8s

import (
	"context"
	"log"
	"time"

	corev1 "k8s.io/api/core/v1"
	"k8s.io/client-go/informers"
	"k8s.io/client-go/kubernetes"
	"k8s.io/client-go/tools/cache"
)

type SecretController struct {
	client    *kubernetes.Clientset
	informer  cache.SharedIndexInformer
	registry  *Registry
	hasSynced cache.InformerSynced
}

func NewSecretController(client *kubernetes.Clientset, registry *Registry) *SecretController {
	factory := informers.NewSharedInformerFactory(client, 30*time.Second)
	informer := factory.Core().V1().Secrets().Informer()

	c := &SecretController{
		client:    client,
		informer:  informer,
		registry:  registry,
		hasSynced: informer.HasSynced,
	}

	informer.AddEventHandler(cache.ResourceEventHandlerFuncs{
		AddFunc:    c.onAdd,
		UpdateFunc: c.onUpdate,
		DeleteFunc: c.onDelete,
	})

	return c
}

func (c *SecretController) Run(ctx context.Context) {
	log.Println("Starting K8s Secret Controller...")
	go c.informer.Run(ctx.Done())

	if !cache.WaitForCacheSync(ctx.Done(), c.hasSynced) {
		log.Println("Timed out waiting for Secret cache sync")
		return
	}
	log.Println("K8s Secret Controller synced.")
}

func (c *SecretController) onAdd(obj interface{}) {
	c.process(obj)
}

func (c *SecretController) onUpdate(old, new interface{}) {
	c.process(new)
}

func (c *SecretController) onDelete(obj interface{}) {
	s, ok := obj.(*corev1.Secret)
	if !ok {
		return
	}
	// Typically we might want to check if it's a TLS secret.
	// For simplicity, just try to delete if it was stored.
	c.registry.DeleteSecret(s.Name)
}

func (c *SecretController) process(obj interface{}) {
	s, ok := obj.(*corev1.Secret)
	if !ok {
		return
	}

	if s.Type != corev1.SecretTypeTLS {
		return
	}
	
	cert := s.Data["tls.crt"]
	key := s.Data["tls.key"]
	
	if len(cert) > 0 && len(key) > 0 {
		c.registry.UpdateSecret(s.Name, cert, key)
	}
}

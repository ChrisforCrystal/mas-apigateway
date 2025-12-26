package config

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"

	agwv1 "github.com/masallsome/masapigateway/control-plane/pkg/proto"
	"gopkg.in/yaml.v3"
)

func LoadConfig(path string) (*agwv1.ConfigSnapshot, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}

	var dslConfig Config
	if err := yaml.Unmarshal(data, &dslConfig); err != nil {
		return nil, err
	}
	fmt.Printf("DEBUG: Loaded Config DSL: %+v\n", dslConfig)
	if dslConfig.Resources != nil {
		fmt.Printf("DEBUG: Loaded Resources: %+v\n", dslConfig.Resources)
	} else {
		fmt.Println("DEBUG: Loaded Resources is NIL")
	}

	return ToProto(&dslConfig, data), nil
}

func ToProto(dsl *Config, rawData []byte) *agwv1.ConfigSnapshot {
	snapshot := &agwv1.ConfigSnapshot{
		VersionId: GenerateVersion(rawData),
		Listeners: make([]*agwv1.Listener, 0),
		Clusters:  make([]*agwv1.Cluster, 0),
		Routes:    make([]*agwv1.Route, 0),
	}

	if dsl.Resources != nil {
		snapshot.Resources = &agwv1.ExternalResources{
			Redis:     make([]*agwv1.RedisConfig, 0),
			Databases: make([]*agwv1.DatabaseConfig, 0),
		}
		for _, r := range dsl.Resources.Redis {
			snapshot.Resources.Redis = append(snapshot.Resources.Redis, &agwv1.RedisConfig{
				Name:    r.Name,
				Address: r.Address,
			})
		}
		for _, db := range dsl.Resources.Databases {
			snapshot.Resources.Databases = append(snapshot.Resources.Databases, &agwv1.DatabaseConfig{
				Name:             db.Name,
				Type:             db.Type,
				ConnectionString: db.ConnectionString,
			})
		}
	}

	for _, l := range dsl.Listeners {
		listener := &agwv1.Listener{
			Name:    l.Name,
			Address: l.Address,
			Port:    l.Port,
		}
		if l.Tls != nil {
			listener.Tls = &agwv1.TlsConfig{
				SecretName: l.Tls.SecretName,
			}
		}
		snapshot.Listeners = append(snapshot.Listeners, listener)
		
		for _, r := range l.Routes {
			var protoPlugins []*agwv1.Plugin
			for _, p := range r.Plugins {
				protoPlugins = append(protoPlugins, &agwv1.Plugin{
					Name:     p.Name,
					WasmPath: p.WasmPath,
					Config:   p.Config,
				})
			}

			route := &agwv1.Route{
				PathPrefix: r.Match,
				ClusterId:  r.Cluster,
				Plugins:    protoPlugins,
			}
			snapshot.Routes = append(snapshot.Routes, route)
		}
	}

	for _, c := range dsl.Clusters {
		cluster := &agwv1.Cluster{
			Name:      c.Name,
			Endpoints: make([]*agwv1.Endpoint, 0),
		}
		for _, e := range c.Endpoints {
			cluster.Endpoints = append(cluster.Endpoints, &agwv1.Endpoint{
				Address: e.Address,
				Port:    e.Port,
			})
		}
		snapshot.Clusters = append(snapshot.Clusters, cluster)
	}

	return snapshot
}

func GenerateVersion(data []byte) string {
	hash := sha256.Sum256(data)
	return hex.EncodeToString(hash[:])[:8]
}

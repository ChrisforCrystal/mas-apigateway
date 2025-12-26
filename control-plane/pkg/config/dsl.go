package config

// Config represents the root of the configuration file.
type Config struct {
	Version   string     `yaml:"version"`
	Resources *Resources `yaml:"resources,omitempty"`
	Listeners []Listener `yaml:"listeners"`
	Clusters  []Cluster  `yaml:"clusters"`
}

type Resources struct {
	Redis     []RedisConfig    `yaml:"redis"`
	Databases []DatabaseConfig `yaml:"databases"`
}

type RedisConfig struct {
	Name    string `yaml:"name"`
	Address string `yaml:"address"`
}

type DatabaseConfig struct {
	Name             string `yaml:"name"`
	Type             string `yaml:"type"`
	ConnectionString string `yaml:"connection_string"`
}

type Listener struct {
	Name    string `yaml:"name"`
	Address string     `yaml:"address"`
	Port    uint32     `yaml:"port"`
	Tls     *TlsConfig `yaml:"tls"`
	Routes  []Route    `yaml:"routes"`
}

type TlsConfig struct {
	SecretName string `yaml:"secret_name"`
}

type Route struct {
	Match   string   `yaml:"match"`   // e.g. "/api"
	Domain  string   `yaml:"domain"`  // e.g. "example.com"
	Cluster string   `yaml:"cluster"` // Cluster reference
	Plugins []Plugin `yaml:"plugins"`
}

type Plugin struct {
	Name     string            `yaml:"name"`
	WasmPath string            `yaml:"wasm_path"`
	Config   map[string]string `yaml:"config"`
}

type Cluster struct {
	Name      string     `yaml:"name"`
	Endpoints []Endpoint `yaml:"endpoints"`
}

type Endpoint struct {
	Address string `yaml:"address"`
	Port    uint32 `yaml:"port"`
}

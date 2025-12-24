package config

import (
	"log"

	"github.com/fsnotify/fsnotify"
	agwv1 "github.com/masallsome/masapigateway/control-plane/pkg/proto"
)

// Watcher 负责监听本地配置文件的变化。
// 当相关文件发生修改、创建或删除时，它会重新加载配置并通过 updates 通道发出通知。
type Watcher struct {
	configPath string                     // 监听的配置文件路径 (例如 "./config.yaml")
	updates    chan *agwv1.ConfigSnapshot // 配置变更通知通道。当文件变化并解析成功后，新的 snapshot 会通过这里发送出去。
	watcher    *fsnotify.Watcher          // 底层的操作系统文件系统监听器 (基于 inotify/kqueue 等系统调用)
}

// NewWatcher 创建一个新的配置监听器实例。
// 它负责初始化文件系统监控器 (fsnotify) 并准备好用于发送配置更新的通道。
func NewWatcher(path string) (*Watcher, error) {
	// 创建 fsnotify 监听器，用于监听底层文件系统的变化事件
	w, err := fsnotify.NewWatcher()
	if err != nil {
		return nil, err
	}
	
	return &Watcher{
		configPath: path,
		// 创建一个带缓冲的通道 (buffer=10)，用于传递解析后的配置快照。
		// 缓冲区可以防止因消费者处理过慢而阻塞配置的重载流程。
		updates:    make(chan *agwv1.ConfigSnapshot, 10), 
		watcher:    w,
	}, nil
}

func (w *Watcher) Updates() <-chan *agwv1.ConfigSnapshot {
	return w.updates
}

func (w *Watcher) Start() error {
	defer w.watcher.Close()

	// Initial load
	if err := w.reload(); err != nil {
		log.Printf("Error loading initial config: %v", err)
	}

	if err := w.watcher.Add(w.configPath); err != nil {
		return err
	}
	
	log.Printf("Watching config file: %s", w.configPath)

	for {
		select {
		case event, ok := <-w.watcher.Events:
			if !ok {
				return nil
			}
			if event.Has(fsnotify.Write) || event.Has(fsnotify.Create) {
				log.Println("Config file modified:", event.Name)
				if err := w.reload(); err != nil {
					log.Printf("Error reloading config: %v", err)
				}
			}
		case err, ok := <-w.watcher.Errors:
			if !ok {
				return nil
			}
			log.Println("Watcher error:", err)
		}
	}
}

// reload 重新读取配置文件并通知监听者
func (w *Watcher) reload() error {
	// 1. 从磁盘读取并解析最新的 YAML 配置文件，生成 ConfigSnapshot 对象
	snapshot, err := LoadConfig(w.configPath)
	if err != nil {
		return err
	}
	
	// 2. 尝试将新配置发送到 updates 通道
	// 这里使用了 select + default 来实现【非阻塞发送】(Non-blocking Send)。
	select {
	case w.updates <- snapshot:
		// 成功：通道里有空位，snapshot 已被放入通道，等待消费者（AgwServer）来取。
		log.Printf("Config reloaded. Version: %s", snapshot.VersionId)
	default:
		// 失败：通道已满（说明消费者处理太慢，或者根本没人从通道里取数据）。
		// 为了不让 Watcher 自己的逻辑在这里卡死（阻塞），我们选择直接丢弃这次更新（或者打印日志）。
		// 因为 Watcher 还要继续监听下一次文件变化，不能因为没人取信就一直站在信箱门口发呆。
		log.Println("Update channel full, dropping update (or consumer is too slow)")
	}
	return nil
}

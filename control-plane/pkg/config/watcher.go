package config

import (
	"log"

	"github.com/fsnotify/fsnotify"
	agwv1 "github.com/masallsome/masapigateway/control-plane/pkg/proto"
)

type Watcher struct {
	configPath string
	updates    chan *agwv1.ConfigSnapshot
	watcher    *fsnotify.Watcher
}

func NewWatcher(path string) (*Watcher, error) {
	w, err := fsnotify.NewWatcher()
	if err != nil {
		return nil, err
	}
	
	return &Watcher{
		configPath: path,
		updates:    make(chan *agwv1.ConfigSnapshot, 10), // Buffer updates
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

func (w *Watcher) reload() error {
	snapshot, err := LoadConfig(w.configPath)
	if err != nil {
		return err
	}
	
	// Non-blocking send if possible, or blocking if critical
	// For simplicity, blocking send but with select to avoid stall if no one listening yet?
	// But updates channel is buffered.
	select {
	case w.updates <- snapshot:
		log.Printf("Config reloaded. Version: %s", snapshot.VersionId)
	default:
		log.Println("Update channel full, dropping update (or consumer is too slow)")
	}
	return nil
}

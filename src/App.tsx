import React, { useState } from 'react';
import { FolderGit2, Code2, Terminal, ChevronRight, ChevronDown, Download, FileJson, FileCode } from 'lucide-react';

const RUST_CODE_TREE = [
  {
    name: 'core_domain',
    type: 'folder',
    children: [
      { name: 'src', type: 'folder', children: [{ name: 'lib.rs', type: 'file' }] },
      { name: 'Cargo.toml', type: 'file' },
    ]
  },
  {
    name: 'git_adapter',
    type: 'folder',
    children: [
      { name: 'src', type: 'folder', children: [{ name: 'lib.rs', type: 'file' }] },
      { name: 'Cargo.toml', type: 'file' },
    ]
  },
  {
    name: 'gui_app',
    type: 'folder',
    children: [
      { name: 'src', type: 'folder', children: [{ name: 'main.rs', type: 'file' }] },
      { name: 'Cargo.toml', type: 'file' },
    ]
  },
  {
    name: 'keyring_adapter',
    type: 'folder',
    children: [
      { name: 'src', type: 'folder', children: [{ name: 'lib.rs', type: 'file' }] },
      { name: 'Cargo.toml', type: 'file' },
    ]
  },
  {
    name: 'ssh_adapter',
    type: 'folder',
    children: [
      { name: 'src', type: 'folder', children: [{ name: 'lib.rs', type: 'file' }] },
      { name: 'Cargo.toml', type: 'file' },
    ]
  },
  {
    name: 'auth_adapter',
    type: 'folder',
    children: [
      { name: 'src', type: 'folder', children: [{ name: 'lib.rs', type: 'file' }] },
      { name: 'Cargo.toml', type: 'file' },
    ]
  },
  {
    name: 'config_adapter',
    type: 'folder',
    children: [
      { name: 'src', type: 'folder', children: [{ name: 'lib.rs', type: 'file' }] },
      { name: 'Cargo.toml', type: 'file' },
    ]
  },
  { name: 'Cargo.toml', type: 'file' },
];

export default function App() {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({
    'core_domain': false,
    'core_domain/src': false,
    'git_adapter': false,
    'git_adapter/src': false,
    'gui_app': false,
    'gui_app/src': false,
    'keyring_adapter': false,
    'keyring_adapter/src': false,
    'ssh_adapter': false,
    'ssh_adapter/src': false,
    'auth_adapter': false,
    'auth_adapter/src': false,
    'config_adapter': true,
    'config_adapter/src': true,
  });

  const toggleFolder = (path: string) => {
    setExpanded(prev => ({ ...prev, [path]: !prev[path] }));
  };

  const renderTree = (items: any[], path = '') => {
    return items.map((item, index) => {
      const currentPath = path ? `${path}/${item.name}` : item.name;
      const isExpanded = expanded[currentPath];

      if (item.type === 'folder') {
        return (
          <div key={currentPath} className={path ? "ml-4" : ""}>
            <div 
              className="flex items-center gap-2 py-1.5 px-2 cursor-pointer hover:bg-[#1A1B1E] text-[#D1D1D1] transition-colors border-l-2 border-transparent hover:border-[#F27D26]"
              onClick={() => toggleFolder(currentPath)}
            >
              {isExpanded ? <ChevronDown className="w-3.5 h-3.5 opacity-50" /> : <ChevronRight className="w-3.5 h-3.5 opacity-50" />}
              <FolderGit2 className="w-4 h-4 text-[#F27D26]" />
              <span className="font-mono text-xs">{item.name}</span>
            </div>
            {isExpanded && renderTree(item.children, currentPath)}
          </div>
        );
      }

      return (
        <div key={currentPath} className={`flex items-center gap-2 py-1.5 px-2 text-[#5A5A5A] hover:bg-[#1A1B1E] hover:text-[#D1D1D1] cursor-default transition-colors border-l-2 border-transparent ${path ? "ml-8" : "ml-4"}`}>
          {item.name.endsWith('.rs') ? <FileCode className="w-4 h-4 text-blue-400 opacity-80" /> : <FileJson className="w-4 h-4 text-[#5A5A5A]" />}
          <span className="font-mono text-xs">{item.name}</span>
        </div>
      );
    });
  };

  return (
    <div className="flex flex-col h-screen w-full bg-[#0D0D0F] text-[#D1D1D1] font-sans overflow-hidden">
      <header className="h-12 border-b border-[#2D2D30] bg-[#151619] flex items-center px-4 justify-between shrink-0">
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2">
            <div className="w-6 h-6 bg-[#F27D26] rounded flex items-center justify-center text-[#000] font-bold text-xs"><Terminal className="w-3.5 h-3.5" /></div>
            <div className="flex flex-col">
              <span className="font-semibold text-white tracking-tight text-[13px] uppercase">GitCthulhu</span>
            </div>
          </div>
          <nav className="hidden md:flex gap-6 text-[10px] uppercase tracking-widest font-bold ml-6">
            <span className="text-[#F27D26]">Phase 8.5: Reactive Configuration (Hot-Reloading)</span>
          </nav>
        </div>
        <div className="flex items-center gap-3">
          <div className="px-3 py-1 bg-[#232429] border border-[#3A3B40] rounded text-[10px] font-mono flex items-center gap-2">
            <div className="w-2 h-2 rounded-full bg-green-500 shadow-[0_0_8px_rgba(34,197,94,0.6)]"></div>
            HEXAGONAL ARCHITECTURE
          </div>
          <div className="px-3 py-1 bg-[#232429] border border-[#3A3B40] rounded text-[10px] font-mono flex items-center gap-2 hidden sm:flex">
            <div className="w-2 h-2 rounded-full bg-[#F27D26] shadow-[0_0_8px_rgba(242,125,38,0.6)]"></div>
            RUST ALL THE WAY
          </div>
        </div>
      </header>

      <main className="flex-1 flex overflow-hidden">
        
        <aside className="w-64 md:w-80 border-r border-[#2D2D30] bg-[#111214] flex flex-col shrink-0">
          <div className="p-4 flex-1 flex flex-col overflow-y-auto">
            <div className="mb-6">
              <h2 className="text-[10px] font-bold mb-3 text-[#F27D26] uppercase tracking-widest flex items-center gap-2">
                <FolderGit2 className="w-4 h-4" />
                Workspace Tree
              </h2>
              <div className="-ml-2">
                {renderTree(RUST_CODE_TREE)}
              </div>
            </div>

            <div className="mt-auto">
              <div className="bg-[#1A1B1E] p-3 rounded-lg border border-[#2D2D30]">
                <p className="text-[10px] uppercase opacity-50 mb-2 font-bold tracking-widest text-white">Environment Note</p>
                <p className="text-[11px] font-mono text-[#D1D1D1] mb-2 leading-relaxed">You are currently viewing this in a sandboxed web preview environment.</p>
                <p className="text-[10px] font-mono opacity-60 leading-relaxed">The native Rust workspace files have been written directly to the filesystem. To compile and run this native desktop app using `cargo` and `iced`, export the project.</p>
                <div className="w-full h-1 bg-[#2D2D30] mt-3 rounded-full overflow-hidden">
                  <div className="w-1/3 h-full bg-[#F27D26]"></div>
                </div>
              </div>
            </div>
          </div>
        </aside>

        <section className="flex-1 bg-[#0D0D0F] relative overflow-hidden flex flex-col min-w-0">
          <div className="h-10 border-b border-[#2D2D30] flex items-center px-4 bg-[#111214] justify-between shrink-0">
            <div className="flex items-center gap-2">
              <Code2 className="w-4 h-4 text-[#F27D26]" />
              <span className="text-[11px] font-mono text-[#D1D1D1]">config_adapter/src/lib.rs</span>
            </div>
            <div className="flex gap-2">
              <span className="px-3 py-1 bg-[#2D2D30] text-[#5A5A5A] text-[10px] font-bold uppercase rounded border border-[#3A3B40]">Read Only</span>
            </div>
          </div>
          
          <div className="flex-1 overflow-auto p-4 bg-[#0D0D0F]">
            <pre className="text-[11px] font-mono text-[#D1D1D1] leading-relaxed"><code>{`//! config_adapter
//! Implements the ConfigStore port for persisting user-defined settings using confy.

use core_domain::{AppSettings, ConfigStore, DomainError};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct AppSettingsDto {
    pub theme: String,
    pub repository_shortcuts: Vec<String>,
}

impl Default for AppSettingsDto {
    fn default() -> Self {
        Self {
            theme: "DeepFocus".to_string(),
            repository_shortcuts: vec![],
        }
    }
}

pub struct ConfyConfigStore {
    app_name: String,
}

impl ConfyConfigStore {
    pub fn new(app_name: &str) -> Self {
        Self {
            app_name: app_name.to_string(),
        }
    }
}

impl ConfigStore for ConfyConfigStore {
    fn load_settings(&self) -> Result<AppSettings, DomainError> {
        let dto: AppSettingsDto = confy::load(&self.app_name, None)
            .map_err(|e| DomainError::Unknown(format!("Failed to load config: {}", e)))?;
        
        Ok(AppSettings {
            theme: dto.theme,
            repository_shortcuts: dto.repository_shortcuts,
        })
    }

    fn save_settings(&self, settings: &AppSettings) -> Result<(), DomainError> {
        let dto = AppSettingsDto {
            theme: settings.theme.clone(),
            repository_shortcuts: settings.repository_shortcuts.clone(),
        };
        
        confy::store(&self.app_name, None, dto)
            .map_err(|e| DomainError::Unknown(format!("Failed to save config: {}", e)))
    }

    fn watch_settings<F>(&self, callback: F) -> Result<(), DomainError> 
    where
        F: Fn(AppSettings) + Send + 'static
    {
        use notify::{Watcher, RecursiveMode, EventKind};
        use std::sync::mpsc::channel;

        let path = confy::get_configuration_file_path(&self.app_name, None)
            .map_err(|e| DomainError::Unknown(format!("Failed to get config path: {}", e)))?;

        let app_name = self.app_name.clone();

        std::thread::spawn(move || {
            let (tx, rx) = channel();
            let mut watcher = notify::recommended_watcher(tx).unwrap();
            
            watcher.watch(&path, RecursiveMode::NonRecursive).unwrap();

            for res in rx {
                if let Ok(event) = res {
                    if matches!(event.kind, EventKind::Modify(_)) {
                        if let Ok(dto) = confy::load::<AppSettingsDto>(&app_name, None) {
                            let new_settings = AppSettings {
                                theme: dto.theme,
                                repository_shortcuts: dto.repository_shortcuts,
                            };
                            callback(new_settings);
                        }
                    }
                }
            }
        });

        Ok(())
    }
}`}</code></pre>
          </div>
        </section>

      </main>

      <footer className="h-8 bg-[#F27D26] text-[#000] flex items-center px-4 justify-between font-mono text-[10px] font-bold shrink-0">
        <div className="flex gap-4">
          <span>WORKSPACE: RUST GIT GUI</span>
          <span>STATUS: SYNCED</span>
        </div>
        <div className="flex gap-4 hidden sm:flex">
          <span>OS: WEB PREVIEW</span>
          <span>ENGINE: VITE/REACT</span>
        </div>
      </footer>
    </div>
  );
}


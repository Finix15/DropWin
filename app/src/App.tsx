"use client"

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { useFileManagement } from "@/hooks/useFileManagement";
import { closeWindow } from "@/lib/windowUtils";
import { FilePreview, FileWithPath } from "@/types";
import { DialogClose } from "@radix-ui/react-dialog";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ChevronDown, Clipboard, Download, Plus, Settings, X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from 'sonner';
import { getFileExtension } from "./lib/utils";
import { StackedIcons } from "./components/StackedIcons";
import { useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";

const isWindows = navigator.platform.toLowerCase().includes('win');

function App() {
  const dragDepth = useRef(0);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [isDragActive, setIsDragActive] = useState(false);
  const [isDragHandleHovered, setIsDragHandleHovered] = useState(false);
  const { files, addFiles, clearFiles, droppedFiles } = useFileManagement();
  const navigate = useNavigate();

  useEffect(() => {
    const webview = getCurrentWebview();
    const eventOptions = { target: webview.label };
    const unlistenWebviewDrop = webview.onDragDropEvent(async (event) => {
        if (event.payload.type === 'enter' || event.payload.type === 'over') {
          setIsDragActive(true);
        } else if (event.payload.type === 'leave') {
          setIsDragActive(false);
        } else if (event.payload.type === 'drop') {
          setIsDragActive(false);
          droppedFiles();
        }
    });

    // Listen to native Windows drag drops
    const unlistenNativeDrop = listen<{ type: 'Files', data: string[] } | { type: 'Text', data: string } | { type: 'Html', data: string }>('native_drop', async (event) => {
      const payload = event.payload;
      if (payload.type === 'Files') {
        try {
          await invoke('add_files', { files: payload.data });
          await droppedFiles();
        } catch (error) {
          console.error('Failed to add dropped files', error);
          toast.error('Could not add dropped files');
        }
      } else if (payload.type === 'Html') {
        const html = payload.data;
        
        let sourceUrl = '';
        const sourceMatch = html.match(/SourceURL:(.*?)[\r\n]/i);
        if (sourceMatch && sourceMatch[1]) {
          sourceUrl = sourceMatch[1].trim();
        }

        // Try to parse using DOM parser for robustness
        const doc = new DOMParser().parseFromString(html, "text/html");
        const img = doc.querySelector("img");
        
        let rawSrc = '';
        if (img && img.getAttribute("src")) {
          rawSrc = img.getAttribute("src") || '';
        } else {
          // Fallback to regex for background images or malformed tags
          const match = html.match(/src=["'](.*?)["']/i) || html.match(/url\(['"]?(.*?)['"]?\)/i);
          if (match && match[1]) {
            rawSrc = match[1];
          }
        }

        if (rawSrc) {
          let src = rawSrc.replace(/&amp;/g, '&');
          if (src.startsWith('//')) {
            src = 'https:' + src;
          } else if (src.startsWith('/')) {
            if (sourceUrl) {
              try {
                const url = new URL(sourceUrl);
                src = url.origin + src;
              } catch (e) {}
            } else {
              src = 'https://www.iamzub.in' + src; 
            }
          }

          if (src.startsWith('data:image/')) {
            const b64match = src.match(/^data:image\/([a-zA-Z]*);base64,(.*)$/);
            if (b64match && b64match[2]) {
              invoke<string>('save_pasted_data_base64', {
                dataBase64: b64match[2],
                extension: b64match[1] || 'png'
              }).then(path => {
                invoke('add_files', { files: [path] });
                droppedFiles();
              }).catch(err => {
                console.error('Failed to save data URI', err);
                toast.error('Failed to save data image');
              });
              return;
            }
          } else if (src.match(/^https?:\/\//i)) {
            toast.info('Downloading image...');
            invoke<string>('download_image_to_shelf', { url: src })
              .then(path => {
                invoke('add_files', { files: [path] });
                droppedFiles();
              }).catch(err => {
                console.error('Failed to download image from HTML', err);
                toast.error('Could not download image: ' + err);
                // Fallback to text link
                invoke<string>('save_pasted_text', { text: src, extension: 'txt' }).then(p => { invoke('add_files', {files:[p]}); droppedFiles(); });
              });
            return;
          }
        }
        
        // Fallback if no image found in HTML
        invoke<string>('save_pasted_text', {
          text: html,
          extension: 'html'
        }).then(path => {
          invoke('add_files', { files: [path] });
          droppedFiles();
        }).catch(err => console.error('Failed to save dropped HTML', err));
      } else if (payload.type === 'Text') {
        const text = payload.data.trim();
        if (text.startsWith('data:image/')) {
          const match = text.match(/^data:image\/([a-zA-Z]*);base64,(.*)$/);
          if (match && match[2]) {
            invoke<string>('save_pasted_data_base64', {
              dataBase64: match[2],
              extension: match[1] || 'png'
            }).then(path => {
              invoke('add_files', { files: [path] });
              droppedFiles();
            }).catch(err => console.error('Failed to save data URI', err));
            return;
          }
        } else if (text.match(/^https?:\/\//i)) {
          // It's a URL. Let's download it.
          invoke<string>('download_image_to_shelf', { url: text })
            .then(path => {
              invoke('add_files', { files: [path] });
              droppedFiles();
            }).catch(err => {
              console.error('Failed to download image', err);
              // Fallback to text
              invoke<string>('save_pasted_text', {
                text: text,
                extension: 'txt'
              }).then(path => {
                invoke('add_files', { files: [path] });
                droppedFiles();
              });
            });
          return;
        }

        invoke<string>('save_pasted_text', {
          text: text,
          extension: 'txt'
        }).then(path => {
          invoke('add_files', { files: [path] });
          droppedFiles();
        }).catch(err => console.error('Failed to save dropped text', err));
      }
    }, eventOptions);

    const unlistenNativeDragState = listen<boolean>('native_drag_state', (event) => {
      setIsDragActive(event.payload);
    }, eventOptions);

    const unlistenOpacity = listen<number>('drop_opacity_changed', (event) => {
      const opacity = Math.min(100, Math.max(20, event.payload)) / 100;
      document.documentElement.style.setProperty('--drop-opacity', opacity.toString());
    });

    // Set up navigation event listener
    const unlisten = listen<string>("navigate_to", (event) => {
      if (event.payload) {
        navigate(event.payload);
      }
    });

    return () => {
      unlistenWebviewDrop.then(fn => fn());
      unlistenNativeDrop.then(fn => fn());
      unlistenNativeDragState.then(fn => fn());
      unlistenOpacity.then(fn => fn());
      unlisten.then(fn => fn());
    };
  }, [addFiles, navigate, droppedFiles]);

  const handleDragEnter = useCallback((e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    dragDepth.current += 1;
    setIsDragActive(true);
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    dragDepth.current = Math.max(0, dragDepth.current - 1);
    if (dragDepth.current === 0) {
      setIsDragActive(false);
    }
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    e.dataTransfer.dropEffect = 'copy';
  }, []);

  useEffect(() => {
    const handleGlobalPaste = (e: ClipboardEvent) => {
      const items = Array.from(e.clipboardData?.items || []);
      for (const item of items) {
        if (item.type.startsWith('image/')) {
          const file = item.getAsFile();
          if (file) {
            const reader = new FileReader();
            reader.onload = async () => {
              const base64Data = (reader.result as string).split(',')[1];
              let extension = 'png';
              if (file.name && file.name.includes('.')) {
                extension = file.name.split('.').pop()?.toLowerCase() || 'png';
              } else if (item.type) {
                extension = item.type.split('/')[1] || 'png';
              }
              try {
                const path = await invoke<string>('save_pasted_data_base64', { 
                  dataBase64: base64Data,
                  extension
                });
                await invoke('add_files', { files: [path] });
              } catch (err) {
                console.error('Failed to paste image', err);
              }
            };
            reader.readAsDataURL(file);
          }
        } else if (item.type === 'text/plain') {
          item.getAsString((text) => {
            if (!text) return;
            invoke<string>('save_pasted_text', {
              text: text,
              extension: 'txt'
            }).then(path => {
              invoke('add_files', { files: [path] });
            }).catch(err => console.error('Failed to paste text', err));
          });
        }
      }
    };

    window.addEventListener('paste', handleGlobalPaste);
    return () => window.removeEventListener('paste', handleGlobalPaste);
  }, []);

  const handleDrop = useCallback(async (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    dragDepth.current = 0;
    setIsDragActive(false);
    toast.info('Drop received in webview!');

    const nativeFiles = Array.from(e.dataTransfer.files);

    if (isWindows && nativeFiles.length > 0) {
      console.error('Native Windows file drop reached the HTML5 fallback without filesystem paths');
      toast.error('Could not resolve the original file path. Please drop the file again.');
      return;
    }

    const filesWithPath: FilePreview[] = [];
    const virtualFiles: File[] = [];

    nativeFiles.forEach((file, index) => {
      const path = (file as FileWithPath).path;
      if (path) {
        filesWithPath.push({
          id: Date.now() + index,
          name: file.name,
          size: file.size,
          path: path,
          icon: getFileExtension(file.name),
          preview: '',
          type: 'file'
        });
      } else {
        virtualFiles.push(file);
      }
    });

    if (nativeFiles.length > 0) {
      await invoke('mark_drop_received').catch((error) => {
        console.error('Failed to mark native drop as received', error);
      });
    }

    if (filesWithPath.length > 0) {
      addFiles(filesWithPath);
    }

    if (virtualFiles.length > 0) {
      toast.info('Processing virtual files from browser...');
      for (const file of virtualFiles) {
        const reader = new FileReader();
        reader.onload = async () => {
          const base64Data = (reader.result as string).split(',')[1];
          let extension = 'png';
          if (file.name && file.name.includes('.')) {
            extension = file.name.split('.').pop()?.toLowerCase() || 'png';
          } else if (file.type) {
            extension = file.type.split('/')[1] || 'png';
          }
          try {
            const path = await invoke<string>('save_pasted_data_base64', {
              dataBase64: base64Data,
              extension
            });
            await invoke('add_files', { files: [path] });
            droppedFiles();
          } catch (err) {
            console.error('Failed to save dropped virtual file', err);
            toast.error('Failed to save dropped image');
          }
        };
        reader.readAsDataURL(file);
      }
      return; // We handled the virtual files, skip HTML fallback
    }

    if (nativeFiles.length === 0) {
      toast.info('No files dropped, checking for text/html...');
      const html = e.dataTransfer.getData("text/html");
      if (html) {
        await invoke('mark_drop_received').catch((error) => {
          console.error('Failed to mark HTML drop as received', error);
        });
        const match = html.match(/<img.*?src=["'](.*?)["']/i);
        if (match && match[1]) {
          const src = match[1];
          if (src.startsWith('data:image/')) {
            const base64Data = src.split(',')[1];
            const byteCharacters = atob(base64Data);
            const byteNumbers = new Array(byteCharacters.length);
            for (let i = 0; i < byteCharacters.length; i++) {
              byteNumbers[i] = byteCharacters.charCodeAt(i);
            }
            const byteArray = new Uint8Array(byteNumbers);
            try {
              const path = await invoke<string>('save_pasted_data_base64', {
                dataBase64: base64Data,
                extension: 'png'
              });
              await invoke('add_files', { files: [path] });
            } catch (err) {
              console.error('Failed to save dropped base64 image', err);
            }
            return;
          } else {
             try {
                const path = await invoke<string>('download_image_to_shelf', { url: src });
                await invoke('add_files', { files: [path] });
                return;
             } catch(err) {
                console.error('Failed to fetch and save dropped image URL', err);
             }
          }
        }
      }

      let text = e.dataTransfer.getData("text/plain");
      if (!text) {
          text = e.dataTransfer.getData("text");
      }
      if (text) {
        await invoke('mark_drop_received').catch((error) => {
          console.error('Failed to mark text drop as received', error);
        });
        try {
          const path = await invoke<string>('save_pasted_text', {
            text: text,
            extension: 'txt'
          });
          await invoke('add_files', { files: [path] });
        } catch (err) {
          console.error('Failed to drop text', err);
        }
      }
      return;
    }

  }, [addFiles]);

  const openPopup = () => {
    invoke('open_popup_window').catch((err) => console.error(err));
  };

  const handleWindowDrag = useCallback(async (event: React.MouseEvent<HTMLDivElement>) => {
    if (event.button !== 0 || (event.target as HTMLElement).closest('button')) return;
    event.preventDefault();
    event.stopPropagation();

    try {
      await getCurrentWindow().startDragging();
    } catch (error) {
      console.error('Failed to move Drop window:', error);
    }
  }, []);

  const stackedIconsRef = useRef<HTMLDivElement>(null);

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setIsModalOpen(true);
  }, []);

  const openSettings = () => {
    invoke('open_settings_window').catch((err) => console.error(err));
  };

  return (
    <div
      className="drop-content-scale fixed left-0 top-0 focus:outline-none"
      onContextMenu={handleContextMenu}
      tabIndex={0}
      onDragEnter={handleDragEnter}
      onDragLeave={handleDragLeave}
      onDragOver={handleDragOver}
      onDrop={handleDrop}
    >
      <div className="drop-shelf-enter">
        <section
          className={`drop-shelf group relative flex h-full flex-col p-2.5 transition-[background,box-shadow,transform] duration-150 ${isDragActive ? 'drop-shelf--dragging' : ''}`}
        >
        <div
          className="absolute inset-x-0 top-0 z-10 h-[20%]"
          onMouseDown={handleWindowDrag}
          onMouseEnter={() => setIsDragHandleHovered(true)}
          onMouseLeave={() => setIsDragHandleHovered(false)}
          onDragStart={(event) => event.preventDefault()}
        />
        <div
          className="relative z-20 flex h-5 shrink-0 items-center justify-end"
          onMouseDown={handleWindowDrag}
          onMouseEnter={() => setIsDragHandleHovered(true)}
          onMouseLeave={() => setIsDragHandleHovered(false)}
          onDragStart={(event) => event.preventDefault()}
        >
          <div
            className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2"
            onMouseDown={handleWindowDrag}
          >
            <div
              className={`h-1 rounded-full transition-[width,background-color] duration-200 ease-out ${isDragHandleHovered ? 'w-11 bg-white/60' : isDragActive ? 'w-11 bg-sky-300/80' : 'w-9 bg-white/35 group-hover:bg-white/55'}`}
            />
          </div>
          <div className="flex items-center gap-0.5 opacity-0 transition-opacity duration-150 group-hover:opacity-100">
            <button
              type="button"
              aria-label="Open settings"
              className="grid h-5 w-5 place-items-center rounded-full text-white/60 transition-colors hover:bg-white/12 hover:text-white"
              onClick={openSettings}
            >
              <Settings className="h-3 w-3" />
            </button>
            <button
              type="button"
              aria-label="Close Drop"
              className="grid h-5 w-5 place-items-center rounded-full text-white/60 transition-colors hover:bg-red-500/75 hover:text-white"
              onClick={closeWindow}
            >
              <X className="h-3 w-3" />
            </button>
          </div>
        </div>

        <div className="flex min-h-0 flex-1 flex-col items-center justify-center">
          {files.length > 0 ? (
            <div
              key={files.length}
              ref={stackedIconsRef}
              className="drop-stack-pop relative flex h-12 w-12 items-center justify-center"
            >
              <StackedIcons files={files} />
            </div>
          ) : (
            <div className="flex flex-col items-center gap-1.5 text-center">
              <div
                className={`grid h-9 w-9 place-items-center rounded-xl border transition-all duration-150 ${isDragActive ? 'border-sky-300/55 bg-sky-300/15 text-sky-100' : 'border-white/10 bg-white/[0.055] text-white/75'}`}
              >
                {isDragActive ? <Plus className="h-5 w-5" /> : <Download className="h-5 w-5" />}
              </div>
              <div>
                <p className="text-[10px] font-medium leading-tight text-white/90">
                  {isDragActive ? 'Release to add' : 'Drop files here'}
                </p>
                <p className="mt-0.5 text-[8px] leading-tight text-white/40">
                  Files, folders, links or text
                </p>
              </div>
            </div>
          )}
        </div>

        {files.length > 0 && (
          <div className="flex shrink-0 items-center justify-center">
            <button
              type="button"
              onClick={openPopup}
              className="flex h-6 items-center gap-1 rounded-full border border-white/10 bg-white/[0.07] px-2.5 text-[9px] font-medium text-white/75 shadow-sm transition-colors hover:bg-white/[0.13] hover:text-white"
            >
              <span>{files.length} item{files.length !== 1 ? 's' : ''}</span>
              <ChevronDown className="h-3 w-3" />
            </button>
          </div>
        )}
        </section>
      </div>

      <Dialog open={isModalOpen} onOpenChange={setIsModalOpen}>
        <DialogContent
          className="rounded-md p-0 mt-2 w-[90vw]"
          aria-describedby={undefined}
        >
          <DialogTitle className="sr-only">Context Menu</DialogTitle>
          <div className="flex flex-col items-start text-foreground">
            {files.length > 0 ? (
              <>
                {/* <Button 
                  className="w-full text-left justify-start"
                  variant="ghost"
                >
                  <Copy className="h-4 w-4 mr-2" />
                  Copy
                </Button>
                <Button 
                  className="w-full text-left justify-start"
                  variant="ghost"
                >
                  <Clipboard className="h-4 w-4 mr-2 " />
                  Paste
                </Button>
                <div className="w-[90%] h-[1px] bg-foreground mx-[5%]"></div> */}
                <Button
                  className="w-full text-left justify-start hover:bg-secondary transition-colors"
                  variant="ghost"
                  onClick={() => {
                    clearFiles(files.map(file => file.id));
                  }}
                >
                  <X className="h-4 w-4" />
                  Clear
                </Button>
              </>
            ) : (
              <Button
                className="w-full text-left justify-start  hover:bg-secondary transition-colors"
                variant="ghost"
              >
                <Clipboard className="h-4 w-4 mr-2" />
                Paste
              </Button>
            )}
          </div>
          <DialogClose asChild>
          </DialogClose>
        </DialogContent>
      </Dialog>
    </div>
  );
}

export default App;

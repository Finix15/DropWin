import { DynamicFileIcon } from "@/components/FileIcon";
import { Button } from "@/components/ui/button";
import { useFileManagement } from "@/hooks/useFileManagement";
import { beginNativeFileDrag } from "@/lib/fileUtils";
import { invoke } from "@tauri-apps/api/core";
import { List as ListIcon, Grid as GridIcon, Trash2, X } from 'lucide-react';
import React, { useEffect, useState, useCallback, useRef } from "react";
import { Toaster } from "sonner";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import SimpleBar from 'simplebar-react';
import 'simplebar-react/dist/simplebar.min.css';

const PopupWindow: React.FC = () => {
  const { files } = useFileManagement();
  const [selectedFiles, setSelectedFiles] = useState<Set<string>>(new Set());
  const [viewMode, setViewMode] = useState<'list' | 'grid'>('list');
  const [lastSelectedFile, setLastSelectedFile] = useState<string | null>(null);
  const cancelDragRef = useRef<(() => void) | null>(null);

  const handleMouseDown = useCallback((e: React.MouseEvent<HTMLDivElement>, file: any) => {
    if (e.button !== 0) return;
    
    e.stopPropagation();
    let filesToDrag: any[];
    if (selectedFiles.size > 0) {
      filesToDrag = files.filter(f => selectedFiles.has(f.id.toString()));
    } else {
      filesToDrag = [file];
    }
    
    cancelDragRef.current?.();
    cancelDragRef.current = beginNativeFileDrag(filesToDrag, e.clientX, e.clientY);
  }, [files, selectedFiles]);

  useEffect(() => {
    return () => cancelDragRef.current?.();
  }, []);

  useEffect(() => {
    const preventBrowserContextMenu = (event: MouseEvent) => {
      event.preventDefault();
    };
    document.addEventListener('contextmenu', preventBrowserContextMenu);
    return () => document.removeEventListener('contextmenu', preventBrowserContextMenu);
  }, []);

  const handleFileClick = useCallback((fileId: string, event: React.MouseEvent) => {
    setSelectedFiles(prev => {
      const newSet = new Set(prev);
      if (event.shiftKey && lastSelectedFile) {
        const fileIds = files.map(f => f.id.toString());
        const startIndex = fileIds.indexOf(lastSelectedFile);
        const endIndex = fileIds.indexOf(fileId);
        const [start, end] = [Math.min(startIndex, endIndex), Math.max(startIndex, endIndex)];
        for (let i = start; i <= end; i++) {
          newSet.add(fileIds[i]);
        }
      } else if (event.ctrlKey || event.metaKey) {
        if (newSet.has(fileId)) {
          newSet.delete(fileId);
        } else {
          newSet.add(fileId);
        }
      } else {
        newSet.clear();
        newSet.add(fileId);
      }
      return newSet;
    });
    setLastSelectedFile(fileId);
  }, [files, lastSelectedFile]);

  const getTotalSize = (files: any[]): string => {
    const totalBytes = files.reduce((acc, file) => acc + file.size, 0);
    return formatFileSize(totalBytes);
  };

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024 * 1024) {
      return `${(bytes / 1024).toFixed(1)} KB`;
    }
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  const toggleViewMode = () => {
    setViewMode(prev => prev === 'list' ? 'grid' : 'list');
  };

  const handleRemoveFile = useCallback(async (fileId: number) => {
    try {
      await invoke('remove_files', { fileIds: [fileId] });
      const fileIdString = fileId.toString();
      setSelectedFiles(previous => {
        const next = new Set(previous);
        next.delete(fileIdString);
        return next;
      });
      setLastSelectedFile(previous => previous === fileIdString ? null : previous);
    } catch (error) {
      console.error(`Failed to remove file ${fileId} from Drop:`, error);
    }
  }, []);

  const stopRemoveButtonMouseDown = useCallback((event: React.MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.stopPropagation();
  }, []);

  const handleClose = useCallback(() => {
    invoke('close_popup_window').catch((error) => {
      console.error('Failed to close popup window:', error);
    });
  }, []);

  return (
    <div className="file-list-popup fixed inset-0 flex flex-col overflow-hidden rounded-2xl p-4">
      <div className="mb-3 flex shrink-0 items-center justify-between">
        <div className="flex items-center space-x-2">
          {files.length > 0 && (
            <>
              <span className="text-xs text-primary">{files.length} items selected</span>
              <span className="text-xs text-primary">{getTotalSize(files)}</span>
            </>
          )}
        </div>
        <div className="flex space-x-2">
          <ToggleGroup
            type="single"
            value={viewMode}
            onValueChange={toggleViewMode}
            >
            <ToggleGroupItem value="list" className="text-primary">
              <span className="sr-only">List</span>
              <ListIcon className="h-4 w-4" />
            </ToggleGroupItem>
            <ToggleGroupItem value="grid" className="text-primary">
              <span className="sr-only">Grid</span>
              <GridIcon className="h-4 w-4" />
            </ToggleGroupItem>
          </ToggleGroup>
          <Button
            variant="ghost"
            size="icon"
            onClick={handleClose}
            title="Hide file list"
            aria-label="Hide file list"
            className="h-9 w-9 rounded-lg text-zinc-300 hover:bg-white/8 hover:text-white"
          >
            <X className="h-5 w-5" />
          </Button>
        </div>
      </div>
      <SimpleBar id="RSC-Example" className="min-h-0 flex-1">
      <div className="flex flex-col overflow-hidden">
          <div className={` overflow-auto ${viewMode === 'grid' ? 'grid grid-cols-2 gap-1' : 'space-y-1'}`}>
            {files.map(file => (
              <div
                key={file.id}
                className={`
                  ${viewMode === 'list'
                    ? 'flex items-center space-x-2 rounded p-2'
                    : 'flex flex-col items-center rounded p-2'
                  }
                  ${selectedFiles.has(file.id.toString()) ? 'file-list-item--selected' : ''}
                  file-list-item relative cursor-grab active:cursor-grabbing
                `}
                onClick={(e) => handleFileClick(file.id.toString(), e)}
                onMouseDown={(e) => handleMouseDown(e, file)}
              >
                <div className={`
                   flex items-center justify-center overflow-hidden
                  ${viewMode === 'list' ? 'w-8 h-8 flex-shrink-0' : 'w-12 h-12 mb-1'}
                `}>
                  {file.preview ? (
                    <img src={file.preview} alt={file.name} className="w-full h-full object-cover" />
                  ) : (
                    <DynamicFileIcon file={file} />
                  )}
                </div>
                <div className={`
                  ${viewMode === 'list' ? 'flex-grow min-w-0' : 'w-full text-center'}
                `}>
                  <p className="text-xs text-primary font-medium truncate" title={file.name}>{file.name}</p>
                  {viewMode === 'grid' && (
                    <span className="text-[10px] text-primary">{formatFileSize(file.size)}</span>
                  )}
                </div>
                {viewMode === 'list' && (
                  <span className="text-[10px] text-primary flex-shrink-0">{formatFileSize(file.size)}</span>
                )}
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  title={`Remove ${file.name} from Drop`}
                  aria-label={`Remove ${file.name} from Drop`}
                  onMouseDown={stopRemoveButtonMouseDown}
                  onClick={(event) => {
                    event.stopPropagation();
                    void handleRemoveFile(file.id);
                  }}
                  className={`${viewMode === 'list'
                    ? 'h-8 w-8 shrink-0'
                    : 'absolute right-1.5 top-1.5 h-7 w-7'
                  } rounded-lg border border-red-400/15 bg-red-500/10 text-red-300/75 opacity-80 shadow-sm transition-all hover:border-red-400/35 hover:bg-red-500/20 hover:text-red-200 hover:opacity-100 focus-visible:opacity-100`}
                >
                  <Trash2 className={`${viewMode === 'list' ? 'h-4 w-4' : 'h-3.5 w-3.5'}`} strokeWidth={2.25} />
                </Button>
              </div>
            ))}
          </div>
      
      <Toaster />
      </div>
      </SimpleBar>

    </div>
  );
};

export default PopupWindow;

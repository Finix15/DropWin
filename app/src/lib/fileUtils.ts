import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { FilePreview } from "../types.ts";

type DragPreviewLayer =
  | { kind: 'folder' }
  | { kind: 'generic-file' }
  | { kind: 'image'; image: HTMLImageElement; cover: boolean };

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error('Failed to load drag preview image'));
    image.src = src;
  });
}

async function resolveDragPreviewLayer(file: FilePreview): Promise<DragPreviewLayer> {
  if (file.type === 'folder') return { kind: 'folder' };

  try {
    if (file.preview) {
      return { kind: 'image', image: await loadImage(file.preview), cover: true };
    }

    const base64Icon = await invoke<string>('get_file_icon_base64', { filePath: file.path });
    return {
      kind: 'image',
      image: await loadImage(`data:image/png;base64,${base64Icon}`),
      cover: false,
    };
  } catch (error) {
    console.error(`Failed to resolve drag preview for ${file.name}:`, error);
    return { kind: 'generic-file' };
  }
}

function roundedRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
): void {
  ctx.beginPath();
  ctx.roundRect(x, y, width, height, radius);
}

function drawFolderIcon(ctx: CanvasRenderingContext2D, size: number): void {
  const half = size / 2;
  roundedRect(ctx, -half, -half, size, size, size * 0.18);
  ctx.fillStyle = 'rgba(251, 191, 36, 0.16)';
  ctx.fill();

  const left = -size * 0.29;
  const top = -size * 0.18;
  const width = size * 0.58;
  const height = size * 0.4;
  ctx.beginPath();
  ctx.moveTo(left, top + height);
  ctx.lineTo(left, top - size * 0.08);
  ctx.quadraticCurveTo(left, top - size * 0.14, left + size * 0.07, top - size * 0.14);
  ctx.lineTo(left + size * 0.23, top - size * 0.14);
  ctx.lineTo(left + size * 0.3, top - size * 0.05);
  ctx.lineTo(left + width - size * 0.04, top - size * 0.05);
  ctx.quadraticCurveTo(left + width, top - size * 0.05, left + width, top);
  ctx.lineTo(left + width, top + height);
  ctx.quadraticCurveTo(left + width, top + height + size * 0.05, left + width - size * 0.05, top + height + size * 0.05);
  ctx.lineTo(left + size * 0.05, top + height + size * 0.05);
  ctx.quadraticCurveTo(left, top + height + size * 0.05, left, top + height);
  ctx.fillStyle = '#f4c542';
  ctx.fill();
  ctx.strokeStyle = '#fff0a3';
  ctx.lineWidth = Math.max(2, size * 0.035);
  ctx.stroke();
}

function drawGenericFileIcon(ctx: CanvasRenderingContext2D, size: number): void {
  const width = size * 0.52;
  const height = size * 0.66;
  const fold = size * 0.16;
  const left = -width / 2;
  const top = -height / 2;
  ctx.beginPath();
  ctx.moveTo(left, top);
  ctx.lineTo(left + width - fold, top);
  ctx.lineTo(left + width, top + fold);
  ctx.lineTo(left + width, top + height);
  ctx.lineTo(left, top + height);
  ctx.closePath();
  ctx.fillStyle = '#e8edf2';
  ctx.fill();
  ctx.strokeStyle = '#7dd3fc';
  ctx.lineWidth = Math.max(2, size * 0.025);
  ctx.stroke();
}

function drawImageLayer(
  ctx: CanvasRenderingContext2D,
  layer: Extract<DragPreviewLayer, { kind: 'image' }>,
  size: number,
): void {
  const { image, cover } = layer;
  const sourceRatio = image.naturalWidth / image.naturalHeight;
  let width = size;
  let height = size;

  if (!cover) {
    if (sourceRatio > 1) height = size / sourceRatio;
    else width = size * sourceRatio;
  }

  ctx.drawImage(image, -width / 2, -height / 2, width, height);
}

async function captureFilesAsImage(files: FilePreview[]): Promise<string | null> {
  try {
    const targetSize = 256;
    const visibleFiles = files.slice(-5).reverse();
    if (visibleFiles.length === 0) return null;
    const layers = await Promise.all(visibleFiles.map(resolveDragPreviewLayer));
    const canvas = document.createElement('canvas');
    canvas.width = targetSize;
    canvas.height = targetSize;
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;

    const iconSize = targetSize * 0.56;
    const rotations = [0, -7, 6, -11, 10];
    for (let index = layers.length - 1; index >= 0; index--) {
      const layer = layers[index];
      ctx.save();
      ctx.translate(
        targetSize / 2 + (index % 2 === 0 ? index * 2 : -index * 2),
        targetSize / 2 - index * 2,
      );
      ctx.rotate((rotations[index] * Math.PI) / 180);
      if (layer.kind === 'folder') drawFolderIcon(ctx, iconSize);
      else if (layer.kind === 'generic-file') drawGenericFileIcon(ctx, iconSize);
      else drawImageLayer(ctx, layer, iconSize);
      ctx.restore();
    }

    if (files.length > 1) drawCountBadge(ctx, targetSize, files.length);
    return canvas.toDataURL('image/png');
  } catch (error) {
    console.error('Failed to build drag preview image:', error);
    return null;
  }
}

function drawCountBadge(ctx: CanvasRenderingContext2D, canvasSize: number, count: number) {
  const radius = 22;
  const cx = canvasSize - radius - 4;
  const cy = canvasSize - radius - 4;

  // Badge circle
  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, Math.PI * 2);
  ctx.fillStyle = '#3b82f6'; // Blue
  ctx.fill();
  ctx.strokeStyle = 'white';
  ctx.lineWidth = 3;
  ctx.stroke();

  // Badge count text
  const label = count > 99 ? '99+' : String(count);
  ctx.fillStyle = 'white';
  ctx.font = `bold ${count > 9 ? 16 : 20}px -apple-system, sans-serif`;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText(label, cx, cy);
}

// Store drag image data to be used synchronously
let pendingDragImage: string | null = null;
// Store pending files for drag
let pendingFiles: FilePreview[] = [];
let dragImageRequestId = 0;

export const prepareDragImage = async (files: FilePreview[]): Promise<boolean> => {
  const requestId = ++dragImageRequestId;
  const dragImage = await captureFilesAsImage(files);
  if (requestId !== dragImageRequestId) return false;
  pendingDragImage = dragImage;
  return true;
};

export const setPendingFiles = (files: FilePreview[]): void => {
  pendingFiles = files;
};

export const clearPendingFiles = (): void => {
  dragImageRequestId += 1;
  pendingFiles = [];
  pendingDragImage = null;
};

const DRAG_START_THRESHOLD_PX = 5;

export const beginNativeFileDrag = (
  files: FilePreview[],
  startX: number,
  startY: number,
): (() => void) => {
  let active = true;
  let previewReady = false;
  let thresholdPassed = false;
  let started = false;

  const removeListeners = (): void => {
    window.removeEventListener('mousemove', handleMouseMove, true);
    window.removeEventListener('mouseup', handleMouseUp, true);
    window.removeEventListener('blur', cancel, true);
  };

  const cancel = (): void => {
    if (!active) return;
    active = false;
    removeListeners();
    if (!started) clearPendingFiles();
  };

  const startIfReady = (): void => {
    if (!active || !previewReady || !thresholdPassed || started) return;
    started = true;
    active = false;
    removeListeners();
    triggerNativeDrag();
  };

  function handleMouseMove(event: MouseEvent): void {
    if ((event.buttons & 1) === 0) {
      cancel();
      return;
    }

    if (Math.hypot(event.clientX - startX, event.clientY - startY) >= DRAG_START_THRESHOLD_PX) {
      thresholdPassed = true;
      event.preventDefault();
      startIfReady();
    }
  }

  function handleMouseUp(): void {
    cancel();
  }

  setPendingFiles(files);
  window.addEventListener('mousemove', handleMouseMove, true);
  window.addEventListener('mouseup', handleMouseUp, true);
  window.addEventListener('blur', cancel, true);

  void prepareDragImage(files).then((isCurrentRequest) => {
    if (!isCurrentRequest || !active) return;
    previewReady = true;
    startIfReady();
  });

  return cancel;
};

/**
 * Trigger native drag for the pending files.
 * Call this synchronously from mousedown to avoid browser drag conflicts.
 */
export const triggerNativeDrag = (): void => {
  if (pendingFiles.length === 0) {
    return;
  }

  const dragImage = pendingDragImage;
  
  // Clear after use
  const filesToDrag = [...pendingFiles];
  pendingFiles = [];
  pendingDragImage = null;

  // Focus is required before native drag on macOS. Windows can start immediately.
  const window = getCurrentWindow();
  window.setFocus().catch((error) => {
    console.error('Failed to focus window before starting drag:', error);
  });

  const startDrag = async (): Promise<void> => {
    if (filesToDrag.length === 1 && filesToDrag[0].name.startsWith('pasted_') && filesToDrag[0].name.endsWith('.txt')) {
        try {
            const { readTextFile } = await import('@tauri-apps/plugin-fs');
            const text = await readTextFile(filesToDrag[0].path);
            await invoke('start_text_drag', {
                text,
                sourceFileIds: filesToDrag.map(file => file.id),
                dragImage
            });
            return;
        } catch (error) {
            console.error('Failed to read and drag text snippet:', error);
        }
    }

    invoke('start_multi_drag', {
      filePaths: filesToDrag.map(file => file.path),
      sourceFileIds: filesToDrag.map(file => file.id),
      dragImage,
    }).catch((error) => {
      console.error('Failed to start native drag:', error);
    });
  };

  if (navigator.platform.toLowerCase().includes('mac')) {
    globalThis.setTimeout(() => void startDrag(), 25);
  } else {
    void startDrag();
  }
};

/**
 * Legacy drag handlers - kept for compatibility but should not be used with draggable attribute
 */
export const handleDragStart = (e: React.DragEvent<HTMLDivElement>, file: FilePreview) => {
  e.preventDefault();
  e.stopPropagation();

  try {
    const win = getCurrentWindow();
    win.setFocus().catch((error) => {
      console.error('Failed to focus window before starting drag:', error);
    });
    setTimeout(() => {
      invoke('start_multi_drag', { 
        filePaths: [file.path], 
        sourceFileIds: [file.id],
        dragImage: null 
      }).catch((error) => {
        console.error('Failed to start native drag:', error);
      });
    }, 50);
  } catch (error) {
    console.error('Failed to invoke native drag:', error);
  }
};

export const handleMultiFileDragStart = (
  e: React.DragEvent<HTMLDivElement>,
  files: FilePreview[],
  dragSourceElement?: HTMLElement
) => {
  e.preventDefault();
  e.stopPropagation();

  // Use pre-captured drag image if available
  const dragImage = pendingDragImage;
  pendingDragImage = null; // Clear after use

  try {
    const win = getCurrentWindow();
    win.setFocus().catch((error) => {
      console.error('Failed to focus window before starting drag:', error);
    });
    setTimeout(() => {
      invoke('start_multi_drag', {
        filePaths: files.map(file => file.path),
        sourceFileIds: files.map(file => file.id),
        dragImage,
      }).catch((error) => {
        console.error('Failed to start native multi-file drag:', error);
      });
    }, 50);
  } catch (error) {
    console.error('Failed to invoke native multi-file drag:', error);
  }
};

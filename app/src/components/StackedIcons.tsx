import React, { useMemo, useRef, useCallback, useEffect } from 'react';
import { DynamicFileIcon } from './FileIcon';
import { FilePreview } from '@/types';
import { beginNativeFileDrag } from '@/lib/fileUtils';

interface StackedIconsProps {
  files: FilePreview[];
}

export const StackedIcons: React.FC<StackedIconsProps> = ({ files }) => {
  const cancelDragRef = useRef<(() => void) | null>(null);

  const stackedIcons = useMemo(() => {
    return files.slice(-5).reverse().map((file, index) => {
      const rotations = [0, -7, 6, -11, 10];
      const rotation = rotations[index];
      const translateX = index % 2 === 0 ? index * 1.5 : -index * 1.5;
      const translateY = -index * 1.25;
      const zIndex = files.length - index;
    
      return (
        <div
          key={index}
          className="pointer-events-none absolute inset-0 flex items-center overflow-hidden rounded-lg drop-shadow-[0_5px_7px_rgba(0,0,0,0.35)]"
          style={{
            transform: `rotate(${rotation}deg) translate(${translateX}px, ${translateY}px)`,
            zIndex,
          }}
        >
          {file.preview ? (
            <img 
              src={file.preview} 
              alt={file.name} 
              className="h-full w-full rounded-lg object-cover"
              loading="lazy"
            />
          ) : (
            <>
              <DynamicFileIcon file={file} className="h-full w-full rounded-lg" />
            </>
          )}
        </div>
      );
    });
  }, [files]);

  const handleMouseDown = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    cancelDragRef.current?.();
    cancelDragRef.current = beginNativeFileDrag(files, e.clientX, e.clientY);
  }, [files]);

  useEffect(() => {
    return () => cancelDragRef.current?.();
  }, []);

  return (
    <div 
      className="relative h-full w-full cursor-grab active:cursor-grabbing"
      onMouseDown={handleMouseDown}
    >
      {stackedIcons}
    </div>
  );
};

import React, { useState, useEffect } from 'react';
import { FileIcon, Folder } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { FilePreview } from '@/types';

interface DynamicFileIconProps extends React.HTMLAttributes<HTMLDivElement> {
  file: FilePreview
}

export const DynamicFileIcon: React.FC<DynamicFileIconProps> = ({ file, className, ...props }) => {
  const [iconBase64, setIconBase64] = useState<string | null>(null);
  const [isVisible, setIsVisible] = useState(false);
  const iconRef = React.useRef<HTMLDivElement>(null);

  useEffect(() => {
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setIsVisible(true);
          observer.disconnect();
        }
      },
      { threshold: 0.1 }
    );

    if (iconRef.current) {
      observer.observe(iconRef.current);
    }

    return () => {
      observer.disconnect();
    };
  }, []);

  useEffect(() => {
    if (file.type === 'folder') {
      setIconBase64(null);
      return;
    }
    if (isVisible) {
      let cancelled = false;
      setIconBase64(null);
      const fetchIcon = async () => {
        try {
          const base64Icon = await invoke<string>('get_file_icon_base64', { filePath: file.path });
          if (!cancelled) {
            setIconBase64(base64Icon);
          }
        } catch (error) {
          console.error('Error fetching file icon:', error);
        }
      };

      fetchIcon();
      return () => {
        cancelled = true;
      };
    }
  }, [isVisible, file.path, file.type]);

  return (
    <div ref={iconRef} className={`h-full w-full ${className ?? ''}`} {...props}>
      {file.type === 'folder' ? (
        <div className="grid h-full w-full place-items-center rounded-[inherit] bg-amber-300/10">
          <Folder className="h-8 w-8 fill-amber-300/80 text-amber-200" />
        </div>
      ) : iconBase64 ? (
        <img className="h-full w-full rounded-[inherit] object-contain" src={`data:image/png;base64,${iconBase64}`} alt="File icon" />
      ) : (
        <div className="grid h-full w-full place-items-center rounded-[inherit] bg-white/10">
          <FileIcon className="h-7 w-7 text-sky-300" />
        </div>
      )}
    </div>
  );
};

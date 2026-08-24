import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { ask, open } from '@tauri-apps/plugin-dialog';
import { X, Plus, Trash2, Keyboard, Monitor, Settings, Info, AlertTriangle, Palette, Languages, FolderOpen } from 'lucide-react';
import { createPortal } from 'react-dom';
import { useCallback, useEffect, useId, useRef, useState } from 'react';
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"

const isMac = navigator.platform.toLowerCase().includes('mac');
type Language = 'en' | 'vi';
type DropSize = 'small' | 'medium' | 'large';

const translations = {
    en: {
        loading: 'Loading settings...', settings: 'Settings', settingsDescription: 'Manage your application preferences',
        permissionTitle: 'Input Monitoring Permission Required', permissionDescription: 'To use global shortcuts like Option+Space or Ctrl+Space, this app needs Input Monitoring permission. This is a macOS system requirement.', permissionOpen: 'Open System Settings', permissionRestart: 'After granting permission, restart the app for changes to take effect.',
        general: 'General Settings', generalDescription: 'Configure startup and language behavior', language: 'Language', languageDescription: 'Language used in the Settings window', languageInfo: 'Changes the Settings interface language immediately. Click Save Settings to use the selected language the next time you open it.',
        autostart: 'Start on System Startup', autostartDescription: 'Automatically launch the app when you sign in', autostartInfo: 'Starts DropWin in the background after you sign in, so shake detection, the tray menu, and the global hotkey are ready without opening the app manually.',
        appearance: 'Drop Appearance', appearanceDescription: 'Customize the floating shelf background', opacity: 'Background Opacity', opacityDescription: 'Updates every open Drop while you drag the slider', opacityInfo: 'Lower values make the Drop background more transparent. Open Drops update immediately; new Drops use the value saved here.', dropPreview: 'Drop files here',
        shortcuts: 'Shortcuts', shortcutsDescription: 'Customize how you interact with the app', hotkey: 'Show Window Hotkey', hotkeyInfo: 'A system-wide shortcut that creates a new Drop and brings it to the foreground, even while another app is active.', noneSet: 'None set', stop: 'Stop', set: 'Set', clear: 'Clear', pressKeys: 'Press desired key combination... Press Stop when done', hotkeyDescription: 'Global shortcut to create a new Drop',
        dragTitle: 'Drag & Drop Behavior', dragDescription: 'How files are transferred when dragging out of DropWin', dragInfo: 'A normal drag copies the selected items. Hold Shift while releasing the mouse to request a move instead.', drag: 'drag', copyFiles: 'Copy files to destination', shift: 'shift', moveFilesPrefix: 'Hold', moveFilesMiddle: 'Shift', moveFilesSuffix: 'while dropping to move files', macDragHint: 'macOS shows a green + badge when copying. No badge = move operation.', windowsDragHint: 'Windows Explorer shows a dotted overlay when copying. A move arrow shows for move operation.',
        mouseMonitor: 'Mouse Monitor', mouseDescription: 'Fine-tune shake detection sensitivity', sensitivity: 'Sensitivity', timing: 'Timing', requiredShakes: 'Required Shakes', requiredShakesInfo: 'Number of left-right direction changes required before DropWin creates a Drop. A lower value triggers more easily.', shakeThreshold: 'Shake Threshold', shakeThresholdInfo: 'Minimum cursor travel, in pixels, counted as one shake. Lower values make small movements more sensitive.', timeLimit: 'Time Limit (ms)', timeLimitInfo: 'Maximum time allowed to complete the required shakes. After this duration, the current shake sequence resets.', blacklist: 'Blacklisted Apps', blacklistDescription: 'Shake detection is disabled while one of these processes is focused. Names are case-insensitive and .exe is optional.', blacklistInfo: 'DropWin works in every app by default. Add an executable such as Photoshop.exe here to disable shake detection only in that app.', dropExecutable: 'Drop an .exe file or Windows shortcut (.lnk) here', addProcess: 'Add executable name...', add: 'Add', noApps: 'No apps blacklisted', allApps: 'Shake detection is enabled in all apps',
        saving: 'Saving...', save: 'Save Settings', infoLabel: 'More information',
    },
    vi: {
        loading: 'Đang tải cài đặt...', settings: 'Cài đặt', settingsDescription: 'Quản lý các tùy chọn của ứng dụng',
        permissionTitle: 'Cần quyền Giám sát đầu vào', permissionDescription: 'Để dùng phím tắt toàn hệ thống như Option+Space hoặc Ctrl+Space, ứng dụng cần quyền Giám sát đầu vào. Đây là yêu cầu của macOS.', permissionOpen: 'Mở Cài đặt hệ thống', permissionRestart: 'Sau khi cấp quyền, hãy khởi động lại ứng dụng để thay đổi có hiệu lực.',
        general: 'Cài đặt chung', generalDescription: 'Thiết lập ngôn ngữ và hành vi khởi động', language: 'Ngôn ngữ', languageDescription: 'Ngôn ngữ dùng trong cửa sổ Cài đặt', languageInfo: 'Thay đổi ngôn ngữ giao diện Cài đặt ngay lập tức. Nhấn Lưu cài đặt để dùng ngôn ngữ đã chọn trong lần mở sau.',
        autostart: 'Khởi động cùng hệ thống', autostartDescription: 'Tự động mở ứng dụng khi bạn đăng nhập', autostartInfo: 'Khởi chạy DropWin dưới nền sau khi đăng nhập để tính năng lắc chuột, menu khay hệ thống và phím tắt luôn sẵn sàng.',
        appearance: 'Giao diện Drop', appearanceDescription: 'Tùy chỉnh nền của ô Drop nổi', opacity: 'Độ trong suốt của nền', opacityDescription: 'Cập nhật mọi Drop đang mở khi kéo thanh trượt', opacityInfo: 'Giá trị thấp làm nền Drop trong suốt hơn. Drop đang mở được cập nhật ngay; Drop mới dùng giá trị đã lưu tại đây.', dropPreview: 'Thả file vào đây',
        shortcuts: 'Phím tắt', shortcutsDescription: 'Tùy chỉnh cách tương tác với ứng dụng', hotkey: 'Phím tắt tạo Drop', hotkeyInfo: 'Phím tắt toàn hệ thống để tạo một Drop mới và đưa nó lên trước, kể cả khi bạn đang dùng ứng dụng khác.', noneSet: 'Chưa đặt', stop: 'Dừng', set: 'Đặt', clear: 'Xóa', pressKeys: 'Nhấn tổ hợp phím mong muốn... Nhấn Dừng khi hoàn tất', hotkeyDescription: 'Phím tắt toàn hệ thống để tạo một Drop mới',
        dragTitle: 'Hành vi kéo và thả', dragDescription: 'Cách file được chuyển khi kéo ra khỏi DropWin', dragInfo: 'Kéo bình thường sẽ sao chép mục đã chọn. Giữ Shift khi thả chuột để yêu cầu di chuyển.', drag: 'kéo', copyFiles: 'Sao chép file đến vị trí đích', shift: 'shift', moveFilesPrefix: 'Giữ', moveFilesMiddle: 'Shift', moveFilesSuffix: 'khi thả để di chuyển file', macDragHint: 'macOS hiện dấu + màu xanh khi sao chép. Không có dấu = thao tác di chuyển.', windowsDragHint: 'Windows Explorer hiện lớp nét chấm khi sao chép. Mũi tên di chuyển xuất hiện khi di chuyển.',
        mouseMonitor: 'Theo dõi chuột', mouseDescription: 'Tinh chỉnh độ nhạy nhận diện thao tác lắc', sensitivity: 'Độ nhạy', timing: 'Thời gian', requiredShakes: 'Số nhịp lắc yêu cầu', requiredShakesInfo: 'Số lần đổi hướng trái-phải cần thiết trước khi DropWin tạo Drop. Giá trị thấp sẽ dễ kích hoạt hơn.', shakeThreshold: 'Ngưỡng lắc', shakeThresholdInfo: 'Quãng đường con trỏ tối thiểu, tính bằng pixel, để được tính là một nhịp lắc. Giá trị thấp nhạy hơn với chuyển động nhỏ.', timeLimit: 'Giới hạn thời gian (ms)', timeLimitInfo: 'Thời gian tối đa để hoàn thành đủ số nhịp lắc. Quá thời gian này, chuỗi lắc hiện tại sẽ được đặt lại.', blacklist: 'Ứng dụng bị chặn', blacklistDescription: 'Tính năng lắc bị tắt khi một trong các tiến trình này đang được chọn. Không phân biệt hoa thường và có thể bỏ đuôi .exe.', blacklistInfo: 'DropWin mặc định hoạt động trong mọi ứng dụng. Thêm file thực thi như Photoshop.exe để chỉ tắt nhận diện lắc trong ứng dụng đó.', dropExecutable: 'Thả file .exe hoặc shortcut Windows (.lnk) vào đây', addProcess: 'Thêm tên file thực thi...', add: 'Thêm', noApps: 'Chưa chặn ứng dụng nào', allApps: 'Tính năng lắc chuột hoạt động trong mọi ứng dụng',
        saving: 'Đang lưu...', save: 'Lưu cài đặt', infoLabel: 'Thông tin chi tiết',
    },
} as const;


interface MouseMonitorConfig {
    required_shakes: number;
    shake_time_limit: number;
    shake_threshold: number;
    blacklist: string[];
}

interface AppConfig {
    mouse_monitor: MouseMonitorConfig;
    autostart: boolean;
    hotkey: string;
    drop_opacity: number;
    language: Language;
    drop_size: DropSize;
}

function InfoTip({ text, label }: { text: string; label: string }) {
    const buttonRef = useRef<HTMLButtonElement>(null);
    const tooltipId = useId();
    const [visible, setVisible] = useState(false);
    const [position, setPosition] = useState({ left: 12, top: 12, above: false, width: 256 });

    const showTooltip = () => {
        const button = buttonRef.current;
        if (!button) return;

        const rect = button.getBoundingClientRect();
        const viewportMargin = 12;
        const tooltipWidth = Math.min(256, window.innerWidth - viewportMargin * 2);
        const centeredLeft = rect.left + rect.width / 2 - tooltipWidth / 2;
        const left = Math.min(
            Math.max(viewportMargin, centeredLeft),
            window.innerWidth - tooltipWidth - viewportMargin,
        );
        const estimatedTooltipHeight = 112;
        const above = rect.bottom + 8 + estimatedTooltipHeight > window.innerHeight;

        setPosition({
            left,
            top: above ? rect.top - 8 : rect.bottom + 8,
            above,
            width: tooltipWidth,
        });
        setVisible(true);
    };

    return (
        <span className="relative inline-flex shrink-0 align-middle">
            <button
                ref={buttonRef}
                type="button"
                aria-label={`${label}: ${text}`}
                aria-describedby={visible ? tooltipId : undefined}
                onMouseEnter={showTooltip}
                onMouseLeave={() => setVisible(false)}
                onFocus={showTooltip}
                onBlur={() => setVisible(false)}
                className="inline-flex h-5 w-5 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
                <Info className="h-3.5 w-3.5" />
            </button>
            {visible && createPortal(
                <span
                    id={tooltipId}
                    role="tooltip"
                    style={{
                        left: position.left,
                        top: position.top,
                        width: position.width,
                        transform: position.above ? 'translateY(-100%)' : undefined,
                    }}
                    className="pointer-events-none fixed z-[9999] rounded-lg border border-border bg-popover px-3 py-2 text-left text-xs font-normal leading-relaxed text-popover-foreground shadow-xl animate-in fade-in-0 zoom-in-95 duration-150"
                >
                    {text}
                </span>,
                document.body,
            )}
        </span>
    );
}

export default function SettingsPage() {
    const [config, setConfig] = useState<AppConfig | null>(null);
    const runtimeDropSizeRef = useRef<DropSize>('small');
    const [saving, setSaving] = useState(false);
    const [isListening, setIsListening] = useState(false);
    const [currentHotkey, setCurrentHotkey] = useState<string>('');
    const [newBlacklistItem, setNewBlacklistItem] = useState('');
    const [platform] = useState(isMac ? 'mac' : 'win');
    const [inputMonitoringGranted, setInputMonitoringGranted] = useState<boolean | null>(null);
    const [isBlacklistDragActive, setIsBlacklistDragActive] = useState(false);
    const [isBrowsingBlacklist, setIsBrowsingBlacklist] = useState(false);
    const blacklistDropZoneRef = useRef<HTMLDivElement>(null);
    const language: Language = config?.language === 'vi' ? 'vi' : 'en';
    const t = translations[language];

    const addBlacklistPaths = useCallback(async (paths: string[]) => {
        try {
            const executables = await invoke<string[]>('resolve_blacklist_executables', { paths });
            if (executables.length === 0) return;
            setConfig((currentConfig) => {
                if (!currentConfig) return currentConfig;
                const blacklist = [...(currentConfig.mouse_monitor.blacklist || [])];
                for (const executable of executables) {
                    if (!blacklist.some((entry) => entry.toLowerCase() === executable.toLowerCase())) {
                        blacklist.push(executable);
                    }
                }
                return {
                    ...currentConfig,
                    mouse_monitor: {
                        ...currentConfig.mouse_monitor,
                        blacklist,
                    },
                };
            });
        } catch (error) {
            console.error('Failed to resolve blacklist executables:', error);
        }
    }, []);

    useEffect(() => {
        loadConfig();
        checkInputMonitoringPermission();

        return () => {
            if (isListening) {
                window.removeEventListener('keydown', handleKeyDown);
                window.removeEventListener('keyup', handleKeyUp);
            }
        };
    }, []);

    useEffect(() => {
        const webview = getCurrentWebview();

        const unlistenNativeDrop = listen<string[]>('settings_native_drop', (event) => {
            setIsBlacklistDragActive(false);
            void addBlacklistPaths(event.payload);
        });
        const unlistenNativeDragState = listen<boolean>('settings_native_drag_state', (event) => {
            setIsBlacklistDragActive(event.payload);
        });

        const unlisten = webview.onDragDropEvent(async (event) => {
            if (event.payload.type === 'leave') {
                setIsBlacklistDragActive(false);
                return;
            }

            const rect = blacklistDropZoneRef.current?.getBoundingClientRect();
            if (!rect) return;
            const position = event.payload.position;
            const scaleFactor = window.devicePixelRatio || 1;
            const x = position.x / scaleFactor;
            const y = position.y / scaleFactor;
            const isInside = x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;

            if (event.payload.type === 'enter' || event.payload.type === 'over') {
                setIsBlacklistDragActive(isInside);
                return;
            }

            setIsBlacklistDragActive(false);
            if (!isInside || event.payload.type !== 'drop') return;

            await addBlacklistPaths(event.payload.paths);
        });

        return () => {
            unlisten.then((stopListening) => stopListening());
            unlistenNativeDrop.then((stopListening) => stopListening());
            unlistenNativeDragState.then((stopListening) => stopListening());
        };
    }, [addBlacklistPaths]);

    const browseBlacklistExecutables = async () => {
        setIsBrowsingBlacklist(true);
        try {
            const selected = await open({
                title: language === 'vi' ? 'Chọn ứng dụng cần chặn' : 'Choose applications to blacklist',
                multiple: true,
                directory: false,
                filters: [{
                    name: 'Windows applications',
                    extensions: ['exe', 'lnk'],
                }],
            });
            const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
            if (paths.length > 0) {
                await addBlacklistPaths(paths);
            }
        } catch (error) {
            console.error('Failed to browse blacklist executables:', error);
        } finally {
            setIsBrowsingBlacklist(false);
        }
    };

    const checkInputMonitoringPermission = async () => {
        if (isMac) {
            try {
                const hasPermission = await invoke<boolean>('plugin:key-intercept|check_permission');
                setInputMonitoringGranted(hasPermission);
            } catch (error) {
                console.error('Failed to check input monitoring permission:', error);
                setInputMonitoringGranted(false);
            }
        } else {
            setInputMonitoringGranted(true);
        }
    };

    const openInputMonitoringSettingsHandler = async () => {
        if (isMac) {
            try {
                await invoke('plugin:key-intercept|open_input_monitoring_settings');
            } catch (error) {
                console.error('Failed to open input monitoring settings:', error);
            }
        }
    };

    const loadConfig = async () => {
        try {
            const [config, runtimeDropSize] = await Promise.all([
                invoke<AppConfig>('get_config'),
                invoke<DropSize>('get_runtime_drop_size'),
            ]);
            runtimeDropSizeRef.current = runtimeDropSize;
            setConfig(config);
        } catch (error) {
            console.error('Failed to load config:', error);
            // Set a default config if loading fails
            setConfig({
                mouse_monitor: {
                    required_shakes: 5,
                    shake_time_limit: 1500,
                    shake_threshold: 100,
                    blacklist: [],
                },
                autostart: false,
                hotkey: '',
                drop_opacity: 88,
                language: 'en',
                drop_size: 'small',
            });
        }
    };

    const saveConfig = async () => {
        if (!config) return;

        const requiresRestart = runtimeDropSizeRef.current !== config.drop_size;
        let saved = false;
        setSaving(true);
        try {
            await invoke('save_config', { newConfig: config });

            try {
                await invoke('set_autostart', { enabled: config.autostart });
            } catch (error) {
                console.error('Failed to update autostart:', error);
            }

            try {
                await invoke('register_hotkey', { shortcutStr: config.hotkey });
            } catch (error) {
                console.error('Failed to register hotkey:', error);
            }
            saved = true;
        } catch (error) {
            console.error('Failed to save config:', error);
        } finally {
            setSaving(false);
        }

        if (!saved || !requiresRestart) return;

        try {
            const restartNow = await ask(
                language === 'vi'
                    ? 'Kích thước Drop mới sẽ được áp dụng sau khi khởi động lại. Khởi động lại ngay? Tất cả ô Drop đang mở sẽ đóng.'
                    : 'The new Drop size will apply after restarting. Restart now? All open Drops will close.',
                {
                    title: language === 'vi' ? 'Cần khởi động lại' : 'Restart required',
                    kind: 'info',
                    okLabel: language === 'vi' ? 'Khởi động lại' : 'Restart',
                    cancelLabel: language === 'vi' ? 'Để sau' : 'Later',
                },
            );

            if (restartNow) {
                await invoke('restart_app');
            }
        } catch (error) {
            console.error('Failed to request app restart:', error);
        }
    };

    const handleClose = async () => {
        await invoke('close_settings_window');
    };

    const updateConfig = (updates: Partial<MouseMonitorConfig>) => {
        if (!config) return;

        setConfig({
            ...config,
            mouse_monitor: {
                ...config.mouse_monitor,
                ...updates,
            },
        });
    };

    const toggleAutostart = () => {
        if (!config) return;

        setConfig({
            ...config,
            autostart: !config.autostart,
        });
    };

    const startKeyListener = () => {
        setIsListening(true);
        setCurrentHotkey('…');
        window.addEventListener('keydown', handleKeyDown, true);
        window.addEventListener('keyup', handleKeyUp, true);
    };

    const stopKeyListener = () => {
        setIsListening(false);
        setCurrentHotkey('');
        window.removeEventListener('keydown', handleKeyDown, true);
        window.removeEventListener('keyup', handleKeyUp, true);
    };

    const clearHotkey = () => {
        if (!config) return;
        setConfig({
            ...config,
            hotkey: '',
        });
    };

    const addBlacklistItem = () => {
        if (!config || !newBlacklistItem.trim()) return;

        const currentBlacklist = config.mouse_monitor.blacklist || [];
        if (!currentBlacklist.includes(newBlacklistItem.trim())) {
            updateConfig({
                blacklist: [...currentBlacklist, newBlacklistItem.trim()]
            });
        }
        setNewBlacklistItem('');
    };

    const removeBlacklistItem = (itemToRemove: string) => {
        if (!config) return;

        const currentBlacklist = config.mouse_monitor.blacklist || [];
        updateConfig({
            blacklist: currentBlacklist.filter(item => item !== itemToRemove)
        });
    };

    const buildHotkeyString = (e: KeyboardEvent): string => {
        const parts: string[] = [];
        const altKeyName = platform === 'mac' ? 'Opt' : 'Alt';

        // Add modifier keys in the correct order
        if (e.ctrlKey) parts.push('Ctrl');
        if (e.altKey) parts.push(altKeyName);
        if (e.shiftKey) parts.push('Shift');
        if (e.metaKey) parts.push('Meta');

        // Get friendly key name for the current key
        let keyName = '';
        const code = e.code;

        // Handle special cases
        if (code.startsWith('Key')) {
            keyName = code.replace('Key', '');
        } else if (code.startsWith('Digit')) {
            keyName = code.replace('Digit', '');
        } else if (code === 'Space') {
            keyName = 'Space';
        } else if (code.startsWith('Arrow')) {
            keyName = code.replace('Arrow', ''); // Up, Down, Left, Right
        } else if (code === 'Escape') {
            keyName = 'Esc';
        } else if (code === 'Backspace') {
            keyName = 'Backspace';
        } else if (code === 'Tab') {
            keyName = 'Tab';
        } else if (code === 'Enter') {
            keyName = 'Enter';
        } else if (code === 'ControlLeft' || code === 'ControlRight') {
            return parts.join('+'); // Only return modifiers for Control key
        } else if (code === 'AltLeft' || code === 'AltRight') {
            return parts.join('+'); // Only return modifiers for Alt key
        } else if (code === 'ShiftLeft' || code === 'ShiftRight') {
            return parts.join('+'); // Only return modifiers for Shift key
        } else if (code === 'MetaLeft' || code === 'MetaRight') {
            return parts.join('+'); // Only return modifiers for Meta key
        } else {
            keyName = code;
        }

        // Add the key name if it's not a modifier key
        if (keyName) {
            parts.push(keyName);
        }

        return parts.join('+');
    };

    const handleKeyDown = (e: KeyboardEvent) => {
        // Always prevent default behavior to stop special character input
        e.preventDefault();
        e.stopPropagation();

        // Update the current hotkey display
        const hotkeyString = buildHotkeyString(e);
        setCurrentHotkey(hotkeyString || '…');

        // Skip if we're only detecting a modifier key press
        if (['ControlLeft', 'ControlRight', 'ShiftLeft', 'ShiftRight',
            'AltLeft', 'AltRight', 'MetaLeft', 'MetaRight'].includes(e.code)) {
            return;
        }

        // If it's a non-modifier key, finalize the hotkey
        if (!['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) {
            // Update config with the new shortcut
            setConfig(prev => prev ? { ...prev, hotkey: hotkeyString } : null);

            // Stop listening for keys
            stopKeyListener();
        }
    };

    const handleKeyUp = (e: KeyboardEvent) => {
        // Update current hotkey display on key up as well
        // (especially important for showing modifier state changes)
        const hotkeyString = buildHotkeyString(e);
        setCurrentHotkey(hotkeyString || '…');
    };

    if (!config) {
        return (
            <div className="settings-surface flex h-full items-center justify-center bg-background text-foreground">
                <div className="flex flex-col items-center space-y-4">
                    <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent"></div>
                    <p className="text-muted-foreground">{t.loading}</p>
                </div>
            </div>
        );
    }

    return (
        <div className="settings-surface flex flex-col h-full bg-background text-foreground select-none">
            <style>{`
                /* Remove number input spinners */
                input[type=number]::-webkit-inner-spin-button, 
                input[type=number]::-webkit-outer-spin-button {
                    -webkit-appearance: none;
                    margin: 0;
                }
                input[type=number] {
                    -moz-appearance: textfield;
                }

                /* Custom scrollbar styling */
                ::-webkit-scrollbar {
                    width: 8px;
                    height: 8px;
                }
                
                ::-webkit-scrollbar-track {
                    background: transparent;
                }
                
                ::-webkit-scrollbar-thumb {
                    background: hsl(var(--border));
                    border-radius: 4px;
                    transition: background 0.2s;
                }
                
                ::-webkit-scrollbar-thumb:hover {
                    background: hsl(var(--primary) / 0.5);
                }
                
                /* Firefox scrollbar styling */
                * {
                    scrollbar-width: thin;
                    scrollbar-color: hsl(var(--border)) transparent;
                }
            `}</style>

            {/* macOS keeps the custom title bar; Windows uses the native title bar. */}
            {isMac && <div className="flex justify-between items-center p-4 border-b border-border/40 bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60 sticky top-0 z-50 min-h-[60px]" data-tauri-drag-region>
                <div className="flex items-center gap-2" data-tauri-drag-region>
                    <div className="bg-primary/10 p-2 rounded-lg" data-tauri-drag-region>
                        <Settings className="h-5 w-5 text-primary" />
                    </div>
                    <div>
                        <h1 className="text-lg font-semibold tracking-tight" data-tauri-drag-region>{t.settings}</h1>
                        <p className="text-xs text-muted-foreground" data-tauri-drag-region>{t.settingsDescription}</p>
                    </div>
                </div>
                <Button
                    variant="ghost"
                    size="icon"
                    onClick={handleClose}
                    className="h-8 w-8 hover:bg-destructive/10 hover:text-destructive rounded-full transition-colors"
                >
                    <X className="h-4 w-4" />
                </Button>
            </div>}

            {/* Settings Content */}
            <div className="flex-grow p-6 overflow-auto space-y-6 scrollbar-thin scrollbar-thumb-border scrollbar-track-transparent">

                {/* macOS Input Monitoring Permission Warning */}
                {isMac && inputMonitoringGranted === false && (
                    <div className="rounded-lg border border-amber-200 bg-amber-50 dark:bg-amber-950/30 dark:border-amber-800 p-4">
                        <div className="flex items-start gap-3">
                            <AlertTriangle className="h-5 w-5 text-amber-600 dark:text-amber-500 shrink-0 mt-0.5" />
                            <div className="flex-1">
                                <h3 className="font-medium text-amber-800 dark:text-amber-200">
                                    {t.permissionTitle}
                                </h3>
                                <p className="text-sm text-amber-700 dark:text-amber-300 mt-1">
                                    {t.permissionDescription}
                                </p>
                                <Button
                                    onClick={openInputMonitoringSettingsHandler}
                                    variant="outline"
                                    className="mt-3 border-amber-300 text-amber-700 hover:bg-amber-100 dark:border-amber-700 dark:text-amber-300 dark:hover:bg-amber-900/50"
                                    size="sm"
                                >
                                    {t.permissionOpen}
                                </Button>
                                <p className="text-xs text-amber-600 dark:text-amber-400 mt-2">
                                    {t.permissionRestart}
                                </p>
                            </div>
                        </div>
                    </div>
                )}

                {/* General Settings */}
                <div className="grid gap-6">
                    <Card>
                        <CardHeader>
                            <CardTitle className="text-base flex items-center gap-2">
                                <Monitor className="w-4 h-4 text-primary" />
                                {t.general}
                            </CardTitle>
                            <CardDescription>{t.generalDescription}</CardDescription>
                        </CardHeader>
                        <CardContent className="grid gap-6">
                            <div className="flex items-center justify-between gap-4">
                                <div className="flex min-w-0 flex-col space-y-1">
                                    <div className="flex items-center gap-1.5">
                                        <Languages className="h-4 w-4 text-muted-foreground" />
                                        <Label>{t.language}</Label>
                                        <InfoTip text={t.languageInfo} label={t.language} />
                                    </div>
                                    <span className="text-xs text-muted-foreground">{t.languageDescription}</span>
                                </div>
                                <div className="inline-flex shrink-0 rounded-lg border border-input bg-muted/30 p-1" role="group" aria-label={t.language}>
                                    {(['vi', 'en'] as const).map((option) => (
                                        <button
                                            key={option}
                                            type="button"
                                            onClick={() => setConfig({ ...config, language: option })}
                                            aria-pressed={language === option}
                                            className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${language === option ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'}`}
                                        >
                                            {option === 'vi' ? 'Tiếng Việt' : 'English'}
                                        </button>
                                    ))}
                                </div>
                            </div>

                            <div className="flex items-center justify-between space-x-2">
                                <div className="flex min-w-0 flex-col space-y-1">
                                    <div className="flex items-center gap-1.5">
                                        <Label htmlFor="startup">{t.autostart}</Label>
                                        <InfoTip text={t.autostartInfo} label={t.autostart} />
                                    </div>
                                    <span className="font-normal text-xs text-muted-foreground">{t.autostartDescription}</span>
                                </div>
                                <Switch
                                    id="startup"
                                    checked={config.autostart}
                                    onCheckedChange={toggleAutostart}
                                />
                            </div>

                        </CardContent>
                    </Card>

                    <Card>
                        <CardHeader>
                            <CardTitle className="text-base flex items-center gap-2">
                                <Palette className="w-4 h-4 text-primary" />
                                {t.appearance}
                            </CardTitle>
                            <CardDescription>{t.appearanceDescription}</CardDescription>
                        </CardHeader>
                        <CardContent className="space-y-4">
                            <div className="flex items-center justify-between gap-4">
                                <div className="flex min-w-0 flex-col space-y-1">
                                    <Label htmlFor="drop-size">
                                        {language === 'vi' ? 'Kích thước ô Drop' : 'Drop size'}
                                    </Label>
                                    <span className="font-normal text-xs text-muted-foreground">
                                        {language === 'vi'
                                            ? 'Áp dụng cho các ô Drop được tạo sau khi lưu'
                                            : 'Applies to Drops created after saving'}
                                    </span>
                                </div>
                                <select
                                    id="drop-size"
                                    value={config.drop_size}
                                    onChange={(event) => setConfig({
                                        ...config,
                                        drop_size: event.target.value as DropSize,
                                    })}
                                    className="h-9 min-w-[8rem] rounded-md border border-input bg-background px-3 text-sm shadow-sm outline-none transition-colors focus-visible:ring-1 focus-visible:ring-ring"
                                >
                                    <option value="small">{language === 'vi' ? 'Bé (100%)' : 'Small (100%)'}</option>
                                    <option value="medium">{language === 'vi' ? 'Vừa (120%)' : 'Medium (120%)'}</option>
                                    <option value="large">{language === 'vi' ? 'Lớn (150%)' : 'Large (150%)'}</option>
                                </select>
                            </div>
                            <div className="flex items-center justify-between gap-4">
                                <div className="flex min-w-0 flex-col space-y-1">
                                    <div className="flex items-center gap-1.5">
                                        <Label htmlFor="drop-opacity">{t.opacity}</Label>
                                        <InfoTip text={t.opacityInfo} label={t.opacity} />
                                    </div>
                                    <span className="font-normal text-xs text-muted-foreground">
                                        {t.opacityDescription}
                                    </span>
                                </div>
                                <span className="min-w-12 rounded-md bg-muted px-2 py-1 text-center font-mono text-sm">
                                    {config.drop_opacity}%
                                </span>
                            </div>
                            <input
                                id="drop-opacity"
                                type="range"
                                min="20"
                                max="100"
                                step="5"
                                value={config.drop_opacity}
                                onChange={(event) => {
                                    const dropOpacity = Number(event.target.value);
                                    setConfig({
                                        ...config,
                                        drop_opacity: dropOpacity,
                                    });
                                    invoke('preview_drop_opacity', { opacity: dropOpacity })
                                        .catch((error) => console.error('Failed to preview Drop opacity:', error));
                                }}
                                className="drop-opacity-slider w-full"
                            />
                            <div className="settings-preview rounded-xl p-4">
                                <div
                                    className="mx-auto flex h-20 max-w-52 items-center justify-center rounded-2xl border border-white/15 text-sm font-medium text-white shadow-xl backdrop-blur-xl"
                                    style={{ backgroundColor: `rgb(23 23 26 / ${config.drop_opacity / 100})` }}
                                >
                                    {t.dropPreview}
                                </div>
                            </div>
                        </CardContent>
                    </Card>

                    <Card>
                        <CardHeader>
                            <CardTitle className="text-base flex items-center gap-2">
                                <Keyboard className="w-4 h-4 text-primary" />
                                {t.shortcuts}
                            </CardTitle>
                            <CardDescription>{t.shortcutsDescription}</CardDescription>
                        </CardHeader>
                        <CardContent className="space-y-4">
                            <div className="space-y-2">
                                <div className="flex items-center gap-1.5">
                                    <Label>{t.hotkey}</Label>
                                    <InfoTip text={t.hotkeyInfo} label={t.hotkey} />
                                </div>
                                <div className="flex gap-2">
                                    <div className={`
                                        flex-1 h-10 px-3 rounded-md border flex items-center justify-center font-mono text-sm shadow-sm transition-colors
                                        ${isListening
                                            ? 'border-primary ring-1 ring-primary bg-primary/5 text-primary'
                                            : 'border-input bg-background text-foreground'
                                        }
                                    `}>
                                        {isListening ? currentHotkey : (config.hotkey || <span className="text-muted-foreground italic">{t.noneSet}</span>)}
                                    </div>
                                    <Button
                                        onClick={isListening ? stopKeyListener : startKeyListener}
                                        variant={isListening ? "destructive" : "default"}
                                        className="w-24 shrink-0 shadow-sm"
                                    >
                                        {isListening ? t.stop : t.set}
                                    </Button>
                                    <Button
                                        onClick={clearHotkey}
                                        variant="outline"
                                        className="w-20 shrink-0 shadow-sm"
                                        disabled={!config.hotkey}
                                    >
                                        {t.clear}
                                    </Button>
                                </div>
                                <p className="text-xs text-muted-foreground flex items-center gap-1.5">
                                    <Info className="w-3 h-3" />
                                    {isListening
                                        ? t.pressKeys
                                        : t.hotkeyDescription}
                                </p>
                            </div>
                        </CardContent>
                    </Card>

                    <Card>
                        <CardHeader>
                            <CardTitle className="text-base flex items-center gap-2">
                                <Info className="w-4 h-4 text-primary" />
                                {t.dragTitle}
                                <InfoTip text={t.dragInfo} label={t.dragTitle} />
                            </CardTitle>
                            <CardDescription>{t.dragDescription}</CardDescription>
                        </CardHeader>
                        <CardContent className="space-y-3">
                            <div className="rounded-md bg-muted/40 border p-3 space-y-2">
                                <div className="flex items-start gap-2">
                                    <kbd className="shrink-0 mt-0.5 inline-flex items-center justify-center rounded border bg-background px-1.5 py-0.5 text-xs font-mono shadow-sm">{t.drag}</kbd>
                                    <span className="text-sm text-foreground">{t.copyFiles}</span>
                                </div>
                                <div className="flex items-start gap-2">
                                    <kbd className="shrink-0 mt-0.5 inline-flex items-center justify-center rounded border bg-background px-1.5 py-0.5 text-xs font-mono shadow-sm">{t.shift}</kbd>
                                    <span className="text-sm text-foreground">{t.moveFilesPrefix} <strong>{t.moveFilesMiddle}</strong> {t.moveFilesSuffix}</span>
                                </div>
                            </div>
                            <p className="text-xs text-muted-foreground flex items-center gap-1.5">
                                <Info className="w-3 h-3 shrink-0" />
                                {platform === 'mac' 
                                    ? t.macDragHint
                                    : t.windowsDragHint}
                            </p>
                        </CardContent>
                    </Card>

                    <Card>
                        <CardHeader>
                            <CardTitle className="text-base flex items-center gap-2">
                                <Monitor className="w-4 h-4 text-primary" />
                                {t.mouseMonitor}
                            </CardTitle>
                            <CardDescription>{t.mouseDescription}</CardDescription>
                        </CardHeader>
                        <CardContent className="space-y-6">
                            <div className="grid grid-cols-2 gap-4">
                                <div className="space-y-2">
                                    <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t.sensitivity}</Label>
                                    <div className="space-y-1">
                                        <div className="flex items-center gap-1.5">
                                            <Label htmlFor="shakes">{t.requiredShakes}</Label>
                                            <InfoTip text={t.requiredShakesInfo} label={t.requiredShakes} />
                                        </div>
                                        <Input
                                            id="shakes"
                                            type="number"
                                            value={config.mouse_monitor.required_shakes}
                                            onChange={(e) => updateConfig({ required_shakes: parseInt(e.target.value) })}
                                            className="font-mono"
                                        />
                                    </div>
                                    <div className="space-y-1">
                                        <div className="flex items-center gap-1.5">
                                            <Label htmlFor="threshold">{t.shakeThreshold}</Label>
                                            <InfoTip text={t.shakeThresholdInfo} label={t.shakeThreshold} />
                                        </div>
                                        <Input
                                            id="threshold"
                                            type="number"
                                            value={config.mouse_monitor.shake_threshold}
                                            onChange={(e) => updateConfig({ shake_threshold: parseInt(e.target.value) })}
                                            className="font-mono"
                                        />
                                    </div>
                                </div>
                                <div className="space-y-2">
                                    <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t.timing}</Label>
                                    <div className="space-y-1">
                                        <div className="flex items-center gap-1.5">
                                            <Label htmlFor="limit">{t.timeLimit}</Label>
                                            <InfoTip text={t.timeLimitInfo} label={t.timeLimit} />
                                        </div>
                                        <Input
                                            id="limit"
                                            type="number"
                                            value={config.mouse_monitor.shake_time_limit}
                                            onChange={(e) => updateConfig({ shake_time_limit: parseInt(e.target.value) })}
                                            className="font-mono"
                                        />
                                    </div>
                                </div>
                            </div>

                            <div
                                ref={blacklistDropZoneRef}
                                className={`space-y-3 rounded-xl border p-3 transition-colors ${isBlacklistDragActive ? 'border-primary bg-primary/10' : 'border-border bg-muted/10'}`}
                            >
                                <div className="space-y-1">
                                    <div className="flex items-center gap-1.5">
                                        <Label>{t.blacklist}</Label>
                                        <InfoTip text={t.blacklistInfo} label={t.blacklist} />
                                    </div>
                                    <p className="text-xs text-muted-foreground">
                                        {t.blacklistDescription}
                                    </p>
                                    <div className="flex items-center justify-between gap-3">
                                        <p className={`text-xs ${isBlacklistDragActive ? 'font-medium text-primary' : 'text-muted-foreground'}`}>
                                            {t.dropExecutable}
                                        </p>
                                        {platform === 'win' && (
                                            <Button
                                                type="button"
                                                variant="outline"
                                                size="sm"
                                                onClick={browseBlacklistExecutables}
                                                disabled={isBrowsingBlacklist}
                                                className="h-8 shrink-0"
                                            >
                                                <FolderOpen className="mr-2 h-4 w-4" />
                                                {language === 'vi' ? 'Chọn file…' : 'Browse…'}
                                            </Button>
                                        )}
                                    </div>
                                </div>

                                <div className="flex gap-2">
                                    <Input
                                        type="text"
                                        value={newBlacklistItem}
                                        onChange={(e) => setNewBlacklistItem(e.target.value)}
                                        placeholder={t.addProcess}
                                        onKeyDown={(e) => {
                                            if (e.key === 'Enter') {
                                                addBlacklistItem();
                                            }
                                        }}
                                        className="font-mono text-sm"
                                    />
                                    <Button
                                        onClick={addBlacklistItem}
                                        variant="secondary"
                                        disabled={!newBlacklistItem.trim()}
                                        className="shrink-0"
                                    >
                                        <Plus className="h-4 w-4 mr-2" />
                                        {t.add}
                                    </Button>
                                </div>

                                <div className="bg-muted/30 rounded-lg border min-h-[100px] max-h-[200px] overflow-y-auto p-1">
                                    {(config.mouse_monitor.blacklist || []).length > 0 ? (
                                        <div className="space-y-1">
                                            {config.mouse_monitor.blacklist.map((app, index) => (
                                                <div
                                                    key={index}
                                                    className="group flex items-center justify-between p-2 rounded-md hover:bg-background hover:shadow-sm hover:border-border/50 border border-transparent transition-all"
                                                >
                                                    <div className="flex items-center gap-2 overflow-hidden">
                                                        <div className="h-6 w-6 rounded bg-primary/10 flex items-center justify-center shrink-0">
                                                            <span className="text-xs font-mono font-bold text-primary">{app.charAt(0).toUpperCase()}</span>
                                                        </div>
                                                        <span className="text-sm font-mono truncate">{app}</span>
                                                    </div>
                                                    <Button
                                                        variant="ghost"
                                                        size="icon"
                                                        onClick={() => removeBlacklistItem(app)}
                                                        title={language === 'vi' ? `Xóa ${app}` : `Remove ${app}`}
                                                        aria-label={language === 'vi' ? `Xóa ${app}` : `Remove ${app}`}
                                                        className="h-9 w-9 shrink-0 rounded-lg border border-destructive/20 bg-destructive/10 text-destructive/80 opacity-80 transition-all hover:border-destructive/40 hover:bg-destructive/20 hover:text-destructive hover:opacity-100 focus-visible:opacity-100"
                                                    >
                                                        <Trash2 className="h-5 w-5" strokeWidth={2.25} />
                                                    </Button>
                                                </div>
                                            ))}
                                        </div>
                                    ) : (
                                        <div className="flex flex-col items-center justify-center py-8 text-center px-4">
                                            <div className="h-8 w-8 rounded-full bg-muted flex items-center justify-center mb-2">
                                                <Monitor className="h-4 w-4 text-muted-foreground" />
                                            </div>
                                            <p className="text-sm font-medium text-foreground">{t.noApps}</p>
                                            <p className="text-xs text-muted-foreground mt-1">{t.allApps}</p>
                                        </div>
                                    )}
                                </div>
                            </div>
                        </CardContent>
                    </Card>
                </div>
            </div>

            {/* Footer */}
            <div className="p-4 border-t border-border bg-background/95 backdrop-blur z-50">
                <Button
                    onClick={saveConfig}
                    disabled={saving}
                    className="w-full sm:w-auto ml-auto px-8 shadow-sm flex items-center gap-2"
                >
                    {saving ? (
                        <>
                            <div className="h-4 w-4 animate-spin rounded-full border-2 border-primary-foreground border-t-transparent"></div>
                            <span>{t.saving}</span>
                        </>
                    ) : (
                        t.save
                    )}
                </Button>
            </div>
        </div>
    );
}

// Simple Switch component helper since it was missing from imports
function Switch({ id, checked, onCheckedChange }: { id: string, checked: boolean, onCheckedChange: () => void }) {
    return (
        <button
            type="button"
            id={id}
            onClick={onCheckedChange}
            className={`
                relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full border-2 border-transparent 
                transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 
                focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50
                ${checked ? 'bg-primary' : 'bg-input'}
            `}
        >
            <span
                className={`
                    pointer-events-none block h-5 w-5 rounded-full bg-background shadow-lg ring-0 transition-transform
                    ${checked ? 'translate-x-5' : 'translate-x-0'}
                `}
            />
        </button>
    );
}

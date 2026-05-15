import { useEffect, useState } from 'react';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import type { OverlayState } from '../lib/types';

const DEFAULT_STATE: OverlayState = {
  state: 'idle',
  title: 'voice-mcp-host',
  subtitle: '',
};

export default function OverlayPanel() {
  const [overlay, setOverlay] = useState<OverlayState>(DEFAULT_STATE);

  useEffect(() => {
    const win = getCurrentWebviewWindow();
    const unlisten = win.listen<OverlayState>('overlay-state', (event) => {
      setOverlay(event.payload);
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  return (
    <div className="overlay-root">
      <div className="overlay-card">
        <div className={`overlay-indicator ${overlay.state}`} />
        <div className="overlay-text">
          <span className="overlay-title">{overlay.title}</span>
          {overlay.subtitle && (
            <span className="overlay-subtitle">{overlay.subtitle}</span>
          )}
        </div>
      </div>
    </div>
  );
}

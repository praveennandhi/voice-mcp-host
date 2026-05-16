import { useEffect, useState, type MouseEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import type { OverlayState } from '../lib/types';

const DEFAULT_STATE: OverlayState = {
  state: 'idle',
  title: '',
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

  useEffect(() => {
    let stopped = false;

    const syncOverlayState = async () => {
      try {
        const next = await invoke<OverlayState>('get_overlay_state');
        if (!stopped) {
          setOverlay(next);
        }
      } catch {
        // The event listener above is still the primary live path.
      }
    };

    syncOverlayState();
    const timer = window.setInterval(syncOverlayState, 150);
    return () => {
      stopped = true;
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    if (overlay.state === 'idle') {
      const win = getCurrentWebviewWindow();
      win.hide();
      const timer = window.setInterval(() => {
        win.hide();
      }, 250);
      return () => window.clearInterval(timer);
    }
  }, [overlay.state]);

  if (overlay.state === 'idle') {
    return <div className="overlay-root" />;
  }

  const statusLabel = {
    recording: 'Listening',
    transcribing: 'Transcribing',
    pasting: 'Inserting',
    ready: 'Inserted',
    error: 'Needs attention',
    idle: '',
  }[overlay.state];

  const startDrag = (event: MouseEvent) => {
    if (event.button !== 0) {
      return;
    }
    invoke('start_overlay_drag').catch((error) => {
      console.error('overlay drag failed', error);
    });
  };

  return (
    <div className="overlay-root" data-tauri-drag-region onMouseDown={startDrag}>
      <div className={`overlay-card overlay-card--${overlay.state}`} data-tauri-drag-region>
        <div className="overlay-grip" aria-hidden="true">
          <span />
          <span />
          <span />
        </div>
        <div className="overlay-mic" aria-hidden="true">
          <span className={`overlay-indicator ${overlay.state}`} />
        </div>
        <div className="overlay-text">
          <span className="overlay-status">{statusLabel}</span>
          <span className="overlay-title">{overlay.title}</span>
          {overlay.subtitle && (
            <span className="overlay-subtitle">{overlay.subtitle}</span>
          )}
        </div>
        <div className="overlay-wave" aria-hidden="true">
          {Array.from({ length: 9 }).map((_, i) => (
            <span key={i} style={{ animationDelay: `${i * 80}ms` }} />
          ))}
        </div>
      </div>
    </div>
  );
}

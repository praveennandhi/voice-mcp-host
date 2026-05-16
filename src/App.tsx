import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import OverlayPanel from './overlay/OverlayPanel';
import SettingsPanel from './settings/SettingsPanel';

const windowLabel = getCurrentWebviewWindow().label;

// Overlay window must have a transparent body so the card floats over the desktop.
if (windowLabel === 'overlay') {
  document.documentElement.classList.add('overlay-window');
  document.body.classList.add('overlay-window');
}

export default function App() {
  if (windowLabel === 'overlay') {
    return <OverlayPanel />;
  }
  return <SettingsPanel />;
}

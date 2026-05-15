import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import OverlayPanel from './overlay/OverlayPanel';
import SettingsPanel from './settings/SettingsPanel';

const windowLabel = getCurrentWebviewWindow().label;

export default function App() {
  if (windowLabel === 'overlay') {
    return <OverlayPanel />;
  }
  return <SettingsPanel />;
}

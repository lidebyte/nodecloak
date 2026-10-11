import {t} from './i18n';
import { useEffect, useState } from 'react';
import { Sun, Moon, Monitor } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { native } from './bridge';
import SelectMenu from './SelectMenu';
import LanguageControl from './LanguageControl';
type Preference = 'system' | 'light' | 'dark';
export default function ThemeControl({visible}:{visible:boolean}) {
  const [preference, setPreference] = useState<Preference>(() => {
    try { const saved = localStorage.getItem('nodecloak.theme'); return saved === 'light' || saved === 'dark' ? saved : 'system'; } catch { return 'system'; }
  });
  useEffect(() => {
    const media = matchMedia('(prefers-color-scheme: dark)');
    const apply = () => {
      const theme = preference === 'system' ? media.matches ? 'dark' : 'light' : preference;
      document.documentElement.dataset.theme = theme;
      document.documentElement.style.colorScheme = theme;
      if (native) void getCurrentWindow().setTheme(preference === 'system' ? null : preference).catch(() => {});
    };
    apply(); media.addEventListener('change', apply);
    try { localStorage.setItem('nodecloak.theme', preference); } catch {}
    return () => media.removeEventListener('change', apply);
  }, [preference]);
  if(!visible)return null;
  return <section className="settings-card interface-settings">
    <label className="settings-preference-row"><Sun size={15}/><span>{t("外观主题")}</span>
      <SelectMenu label={t("外观主题")} value={preference} onChange={value=>setPreference(value as Preference)} options={[
        {value:'light',label:t('浅色主题'),icon:<Sun size={16}/>},
        {value:'dark',label:t('深色主题'),icon:<Moon size={16}/>},
        {value:'system',label:t('跟随系统主题'),icon:<Monitor size={16}/>},
      ]}/>
    </label>
    <LanguageControl/>
  </section>;
}

import {t} from './i18n';
import {Clock3} from 'lucide-react';
import SelectMenu from './SelectMenu';
import { timezoneChoices, timezoneOffsetLabel, type TimezoneTarget, type TimezoneOption } from './domain';

interface Props {
  target: TimezoneTarget; custom: string; options: TimezoneOption[]; loading: boolean;
  error: string; platform: string; disabled: boolean;
  onTarget: (target: TimezoneTarget) => void; onCustom: (zone: string) => void; onReload: () => void;
}

export default function TimezonePicker({ target, custom, options, loading, error, platform, disabled, onTarget, onCustom, onReload }: Props) {
  const chosen = options.find(zone => zone.id === custom);
  return <section className="timezone-picker">
    <label htmlFor="target-timezone">{t("目标系统时区")}</label>
    <SelectMenu id="target-timezone" label={t("目标系统时区")} value={target} disabled={disabled} onChange={value=>onTarget(value as TimezoneTarget)}
      options={Object.entries(timezoneChoices).map(([value,choice])=>({value,label:t(choice.label),icon:<Clock3 size={16}/>}))} />
    {target === 'custom' && <>
      <label htmlFor="custom-timezone" className="custom-timezone-label">{t("搜索或输入指定时区")}</label>
      <SelectMenu id="custom-timezone" label={t("搜索或输入指定时区")} value={custom} disabled={disabled || loading || !options.length} searchable
        placeholder={t(platform === 'Windows' ? '例如 Tokyo Standard Time' : '例如 Asia/Tokyo 或 America/New_York')}
        invalid={!!custom && !chosen} describedBy="timezone-input-status" onChange={onCustom}
        options={options.map(zone=>({value:zone.id,label:`${zone.id} · ${t(timezoneOffsetLabel(zone.offsetMinutes))} · ${zone.label}`,icon:<Clock3 size={16}/>}))} />
      <p id="timezone-input-status" role={error ? 'alert' : 'status'}>
        {t(loading ? '正在读取系统支持的时区…' : error || (chosen ? `已选择 ${chosen.id} · 当前偏移 ${timezoneOffsetLabel(chosen.offsetMinutes)}。支持夏令时的地区会按系统规则自动调整。` : '请从系统支持的清单选择完整时区名称，输入无效时无法应用。'))}
      </p>
      {t(error && <button type="button" className="button secondary small-button" disabled={disabled || loading} onClick={onReload}>{t("重新读取时区")}</button>)}
    </>}
    <p>{t(timezoneChoices[target].consequence)}</p>
  </section>;
}

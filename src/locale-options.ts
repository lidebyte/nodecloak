export const terminalLocaleChoices = [
  ['en_US.UTF-8','English (United States)'],
  ['en_GB.UTF-8','English (United Kingdom)'],
  ['zh_CN.UTF-8','简体中文（中国）'],
  ['zh_TW.UTF-8','繁體中文（台灣）'],
  ['zh_HK.UTF-8','繁體中文（香港）'],
  ['ja_JP.UTF-8','日本語'],
  ['ko_KR.UTF-8','한국어'],
  ['de_DE.UTF-8','Deutsch'],
  ['fr_FR.UTF-8','Français'],
  ['es_ES.UTF-8','Español'],
  ['pt_BR.UTF-8','Português (Brasil)'],
  ['ru_RU.UTF-8','Русский'],
  ['C.UTF-8','C (UTF-8)'],
  ['C','POSIX'],
].map(([value,name])=>({value,label:`${value} · ${name}`}));

const commonTimezones = ['Asia/Singapore','Asia/Shanghai','Asia/Hong_Kong','Asia/Taipei','Asia/Tokyo','Asia/Seoul','Asia/Kolkata','Asia/Dubai','Europe/London','Europe/Paris','Europe/Berlin','America/New_York','America/Chicago','America/Denver','America/Los_Angeles','America/Toronto','America/Sao_Paulo','Australia/Sydney','Pacific/Auckland'];
export function ianaTimezoneChoices() {
  // Older desktop webviews can still select common zones or enter any IANA name.
  const intl=Intl as typeof Intl & {supportedValuesOf?:(key:string)=>string[]};
  let zones:string[]=[];
  try {zones=intl.supportedValuesOf?.('timeZone')??[];} catch { /* Use common zones. */ }
  return [...new Set(['UTC',...commonTimezones,...zones])].map(value=>({value,label:value}));
}

export const browserLanguageChoices = [
  ['en-US,en','English (United States)'],
  ['en-GB,en','English (United Kingdom)'],
  ['en-SG,en','English (Singapore)'],
  ['zh-CN,zh','简体中文（中国）'],
  ['zh-TW,zh','繁體中文（台灣）'],
  ['zh-HK,zh','繁體中文（香港）'],
  ['ja-JP,ja','日本語'],
  ['ko-KR,ko','한국어'],
  ['de-DE,de','Deutsch'],
  ['fr-FR,fr','Français'],
  ['es-ES,es','Español'],
  ['pt-BR,pt','Português (Brasil)'],
  ['ru-RU,ru','Русский'],
].map(([value,name])=>({value,label:`${value} · ${name}`}));

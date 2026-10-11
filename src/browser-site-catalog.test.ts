import {describe,expect,it} from 'vitest';
import {normalizeCatalog,bundledCatalog,catalogIconURL} from './browser-site-catalog';

describe('published browser website catalog',()=>{
  it('validates announcement identity, revisions and timestamps while preserving history',()=>{
    const announcement={id:'news',title:'Notice',body:'Text',revision:2,publishedAt:'2026-10-11T10:00:00Z'};
    expect(normalizeCatalog({...bundledCatalog,announcements:[announcement]})?.announcements).toEqual([announcement]);
    for(const announcements of [[announcement,announcement],[{...announcement,revision:0}],[{...announcement,publishedAt:'invalid'}],[{...announcement,body:' '}]] ) expect(normalizeCatalog({...bundledCatalog,announcements})).toBeNull();
  });
  it('retains published empty catalog and orders only enabled groups and sites',()=>{
    expect(normalizeCatalog({revision:9,groups:[]})?.groups).toEqual([]);
    const first={id:'first',name:'First',nameEn:'First',icon:'ai',order:2,enabled:true,sites:[{id:'hidden',name:'Hidden',url:'https://example.com',icon:'globe',order:0,enabled:false},{id:'visible',name:'Visible',url:'https://example.com',icon:'globe',order:1,enabled:true}]};
    const value=normalizeCatalog({revision:10,groups:[first,{...first,id:'second',order:0,sites:[]},{...first,id:'disabled',enabled:false,sites:[]}]});
    expect(value?.groups.map(g=>g.id)).toEqual(['second','first']);
    expect(value?.groups[1].sites.map(s=>s.id)).toEqual(['visible']);
  });
  it('rejects unsafe URL and icon data instead of passing arbitrary destinations to native launch',()=>{
    for(const url of ['http://example.com','https://user:pass@example.com','https://127.0.0.1','https://10.1.2.3','https://localhost','https://example.com:8443']){
      const value=structuredClone(bundledCatalog);value.groups[0].sites[0].url=url;
      expect(normalizeCatalog(value),url).toBeNull();
    }
    expect(catalogIconURL('https://attacker.example/logo.svg')).toBeNull();
    expect(catalogIconURL('/v1/catalog/icons/'+'a'.repeat(64)+'.png')).toBe('https://nodecloak.com/v1/catalog/icons/'+'a'.repeat(64)+'.png');
    expect(catalogIconURL('/v1/catalog/icons/../admin')).toBeNull();
  });
  it('ships eleven distinct brand icon identifiers with offline catalog',()=>{
    const sites=bundledCatalog.groups.flatMap(g=>g.sites);
    expect(sites).toHaveLength(11);
    expect(new Set(sites.map(s=>s.icon)).size).toBe(11);
    expect(normalizeCatalog(bundledCatalog)).not.toBeNull();
  });
  it('rejects missing identifiers and accepts safe HTTPS anchors',()=>{
    const invalid=structuredClone(bundledCatalog);delete (invalid.groups[0] as Partial<typeof invalid.groups[0]>).id;
    expect(normalizeCatalog(invalid)).toBeNull();
    const anchor=structuredClone(bundledCatalog);anchor.groups[0].sites[0].url='https://example.com/#start';
    expect(normalizeCatalog(anchor)).not.toBeNull();
  });
});

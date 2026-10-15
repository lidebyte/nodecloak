function (args) {
  // Executes only in the controller's isolated world / BiDi sandbox.
  const key='__nodecloakAutomationV1';
  if(args.action==='wait')return {ready:document.readyState==='complete',found:(document.body?.innerText||'').includes(args.text||'')};
  if(args.action==='snapshot') {
    if(typeof args._new_snapshot_id!=='string'||!args._new_snapshot_id)throw Error('SNAPSHOT_ID_REQUIRED');
    const state={document,epoch:performance.timeOrigin,id:args._new_snapshot_id,nodes:new Map()};
    globalThis[key]=state;
    const elements=[];
    for(const el of document.querySelectorAll('a,button,input,textarea,select,[role="button"],[role="link"],[contenteditable="true"]')) {
      const style=getComputedStyle(el),rect=el.getBoundingClientRect();
      if(!rect.width||!rect.height||style.display==='none'||style.visibility==='hidden')continue;
      const ref=String(elements.length+1);state.nodes.set(ref,el);
      elements.push({ref,tag:el.tagName.toLowerCase(),type:el.getAttribute('type'),name:(el.getAttribute('aria-label')||el.getAttribute('placeholder')||el.innerText||el.getAttribute('name')||'').slice(0,300),disabled:!!el.disabled});
      if(elements.length>=500)break;
    }
    return {snapshot_id:state.id,url:location.href,title:document.title,text:(document.body?.innerText||'').slice(0,80000),elements,external_content:true};
  }
  if(args.action==='state')return {url:location.href,title:document.title,epoch:performance.timeOrigin,ready:document.readyState};
  if(args.action==='scroll'){scrollBy({left:args.x||0,top:args.y||0,behavior:'instant'});return {scrolled:true};}
  const state=globalThis[key];
  if(!state||state.document!==document||state.epoch!==performance.timeOrigin||args.snapshot_id!==state.id)throw Error('STALE_SNAPSHOT: read a new snapshot');
  const el=state.nodes.get(args.element_ref);
  if(!el||!el.isConnected)throw Error('STALE_ELEMENT: read a new snapshot');
  if(args.action==='describe')return {url:location.href,title:document.title,epoch:performance.timeOrigin,tag:el.tagName.toLowerCase(),type:el.getAttribute('type'),name:(el.getAttribute('aria-label')||el.getAttribute('placeholder')||el.innerText||el.getAttribute('name')||'').slice(0,300),field:el.getAttribute('name'),href:el.href||null,form_action:el.formAction||el.form?.action||null};
  if(args.action==='node'||args.action==='file_node'){if(args.action==='file_node'&&(el.tagName!=='INPUT'||el.type!=='file'))throw Error('ELEMENT_NOT_FILE_INPUT');if(el.disabled)throw Error('ELEMENT_DISABLED');el.scrollIntoView?.({block:'center',inline:'center'});if(args.action==='node'){const r=el.getBoundingClientRect();const hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(!hit||(hit!==el&&!el.contains(hit)))throw Error('ELEMENT_INTERCEPTED');}return el;}
  if(el.disabled)throw Error('ELEMENT_DISABLED');
  el.scrollIntoView?.({block:'center',inline:'center'});
  if(args.action==='click'){el.click();return {clicked:true};}
  if(args.action==='fill') {
    if(!['INPUT','TEXTAREA'].includes(el.tagName)||['file','hidden','checkbox','radio'].includes(el.type))throw Error('ELEMENT_NOT_TEXT_INPUT');
    if(el.readOnly)throw Error('ELEMENT_READONLY');
    const proto=el.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto,'value').set.call(el,args.value);
    el.dispatchEvent(new Event('input',{bubbles:true}));el.dispatchEvent(new Event('change',{bubbles:true}));return {filled:true};
  }
  if(args.action==='select') {
    if(el.tagName!=='SELECT')throw Error('ELEMENT_NOT_SELECT');
    const values=Array.isArray(args.values)?args.values:[args.value];
    if(!el.multiple&&values.length>1)throw Error('SELECT_NOT_MULTIPLE');
    for(const value of values)if(!Array.from(el.options).some(o=>o.value===value))throw Error('OPTION_NOT_FOUND');
    for(const o of el.options)o.selected=values.includes(o.value);
    el.dispatchEvent(new Event('input',{bubbles:true}));el.dispatchEvent(new Event('change',{bubbles:true}));return {selected:true};
  }
  throw Error('UNKNOWN_PAGE_ACTION');
}

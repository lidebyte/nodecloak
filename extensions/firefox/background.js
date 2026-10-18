/* global browser, nodecloakFingerprint */
'use strict';

// This extension carries code only. The signed package is identical for every
// profile; a narrowly scoped native host supplies that profile's settings.
let registration;
const flags=['canvasNoise','fontNoise','webglNoise','audioNoise','mediaNoise'];
function validSettings(value){
  return value && value.version===1 && value.enabled===true &&
    typeof value.seed==='string' && /^[0-9a-f]{64}$/.test(value.seed) && flags.every(key=>typeof value[key]==='boolean') &&
    Object.keys(value).every(key=>['version','seed','enabled',...flags].includes(key));
}
const ready=(async()=>{
  let timer;
  try {
    const response=await Promise.race([
      browser.runtime.sendNativeMessage('com.nodecloak.fingerprint',{type:'get_settings'}),
      new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error('HOST_TIMEOUT')),10000);}),
    ]);
    if(!validSettings(response?.fingerprint))throw new Error('INVALID_PROFILE_SETTINGS');
    registration=await browser.contentScripts.register({
      matches:['http://*/*','https://*/*'],
      js:[{code:`(${nodecloakFingerprint.toString()})(${JSON.stringify(response.fingerprint)});`}],
      allFrames:true,matchAboutBlank:true,runAt:'document_start',world:'MAIN',
    });
    return true;
  } catch {
    // Never log profile settings, URLs or native messaging payloads.
    console.error('NodeCloak profile privacy could not be initialized.');
    return false;
  } finally {clearTimeout(timer);}
})();

// Register synchronously during background startup. Network documents cannot
// run their first script until their profile's readback protection is ready.
browser.webRequest.onBeforeRequest.addListener(
  async()=>await ready?{}:{cancel:true},
  {urls:['http://*/*','https://*/*'],types:['main_frame','sub_frame']},
  ['blocking'],
);

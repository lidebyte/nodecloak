"""Windows-only, opt-in installed-Firefox extension/native-host integration check.
Uses disposable profiles and refuses to overwrite any native-host registration.
Does not sign into websites or touch saved browser profiles.
"""
import argparse,sys
import http.server,json,pathlib,subprocess,tempfile,threading,queue,time,urllib.parse,socket,os,hashlib
root=pathlib.Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--host-executable',required=True,type=pathlib.Path)
parser.add_argument('--browser-executable',default='C:/Program Files/Mozilla Firefox/firefox.exe',type=pathlib.Path)
args=parser.parse_args()
if sys.platform!='win32':raise SystemExit('This opt-in integration fixture currently supports Windows only.')
from firefox_extension_registry import temporary_native_host
args.host_executable=args.host_executable.resolve(strict=True)
args.browser_executable=args.browser_executable.resolve(strict=True)
if not args.host_executable.is_file() or not args.browser_executable.is_file():raise SystemExit('Executable paths must point to files.')
(root/'.qa').mkdir(exist_ok=True)
reports=queue.Queue()
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  path=urllib.parse.urlsplit(self.path)
  if path.path=='/report':
   reports.put(json.loads(urllib.parse.parse_qs(path.query)['v'][0]));body=b'ok'
  else:
   frame=path.path=='/frame'
   js="""const c=document.createElement('canvas');c.width=180;c.height=45;const x=c.getContext('2d');x.fillStyle='#ff0';x.fillRect(0,0,180,45);x.fillStyle='#123';x.font='16px Arial';x.fillText('NodeCloak privacy fixture',2,22);const empty=document.createElement('iframe');document.documentElement.appendChild(empty);const emptyFrameProtected=!!empty.contentWindow[Symbol.for('nodecloak.readback.v1')];const clean=empty.contentWindow.CanvasRenderingContext2D.prototype.getImageData;const cleanPixels=Reflect.apply(clean,x,[0,0,180,45]);const emptyPixels=Array.from(cleanPixels.data).reduce((h,v)=>Math.imul(h^v,16777619)>>>0,2166136261);fetch('/report?v='+encodeURIComponent(JSON.stringify({frame:FRAME,emptyFrameProtected,emptyPixels,webdriver:navigator.webdriver,marker:!!globalThis[Symbol.for('nodecloak.readback.v1')],canvas:c.toDataURL(),pixels:Array.from(x.getImageData(0,0,180,45).data).reduce((h,v)=>Math.imul(h^v,16777619)>>>0,2166136261),timezone:Intl.DateTimeFormat().resolvedOptions().timeZone})));""".replace('FRAME',str(frame).lower())
   body=('<!doctype html><meta charset="utf-8"><title>Firefox extension fixture</title><script>'+js+'</script>'+('' if frame else '<iframe src="/frame"></iframe>')).encode()
  self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
def read_packet(s):
 header=b''
 while not header.endswith(b':'):
  b=s.recv(1)
  if not b:raise EOFError('RDP connection closed')
  header+=b
  if len(header)>10:raise ValueError('RDP header too long')
 size=int(header[:-1]);data=b''
 if size>4*1024*1024:raise ValueError('RDP packet too large')
 while len(data)<size:
  part=s.recv(size-len(data))
  if not part:raise EOFError('RDP truncated')
  data+=part
 return json.loads(data)
def call(s,to,typ,**args):
 data=json.dumps(dict(to=to,type=typ,**args)).encode();s.sendall(str(len(data)).encode()+b':'+data)
 while True:
  result=read_packet(s)
  if result.get('from')==to:return result
result=[]
manifest_root=tempfile.TemporaryDirectory(prefix='nodecloak-firefox-native-host-')
host_manifest=pathlib.Path(manifest_root.name)/'com.nodecloak.fingerprint.json'
host_manifest.write_text(json.dumps(dict(name='com.nodecloak.fingerprint',description='NodeCloak isolated profile privacy test',path=str(args.host_executable),type='stdio',allowed_extensions=['fingerprint@nodecloak.com'])))
registration=temporary_native_host(host_manifest)
registration.__enter__()
try:
 with tempfile.TemporaryDirectory(prefix='nodecloak-firefox-extension-proof-') as folder:
  base=pathlib.Path(folder);profile=base/'profile';profile.mkdir();addon=base/'addon';addon.mkdir()
  config=dict(version=1,seed='37'*32,enabled=True,canvasNoise=True,fontNoise=True,webglNoise=True,audioNoise=True,mediaNoise=True)
  fingerprint=(root/'src-tauri/src/fingerprint.js').read_text()
  (addon/'fingerprint.js').write_text(fingerprint,encoding='utf-8')
  manifest=json.loads((root/'extensions/firefox/manifest.json').read_text())
  manifest['permissions'].append('tabs')
  (addon/'manifest.json').write_text(json.dumps(manifest))
  for seed in ['37'*32,'37'*32,'a4'*32]:
   config['seed']=seed;(profile/'nodecloak-fingerprint-extension.json').write_text(json.dumps(config),encoding='utf-8')
   (addon/'background.js').write_text((root/'extensions/firefox/background.js').read_text()+"\nready.then(ok=>{if(!ok)throw new Error('Profile privacy startup failed');return browser.tabs.query({active:true,currentWindow:true}).then(t=>browser.tabs.update(t[0].id,{url:"+json.dumps(f'http://127.0.0.1:{server.server_port}/fixture')+"}));});")
   with socket.socket() as reserve:reserve.bind(('127.0.0.1',0));port=reserve.getsockname()[1]
   prefs={'browser.startup.page':0,'browser.startup.homepage_override.mstone':'ignore','devtools.debugger.remote-enabled':True,'devtools.chrome.enabled':True,'devtools.debugger.prompt-connection':False,'devtools.debugger.force-local':True,'privacy.baselineFingerprintingProtection.overrides':'-EfficientCanvasRandomization','privacy.fingerprintingProtection':False}
   (profile/'user.js').write_text('\n'.join('user_pref('+json.dumps(k)+','+json.dumps(v)+');' for k,v in prefs.items()))
   env=os.environ.copy();env['NODECLOAK_FIREFOX_PROFILE']=str(profile)
   with open(root/'.qa/firefox-extension-proof-browser.log','wb') as log:
    process=subprocess.Popen([str(args.browser_executable),'--headless','--no-remote','--profile',str(profile),'--start-debugger-server',str(port),'--new-window','about:blank'],stdout=log,stderr=log,env=env,creationflags=subprocess.CREATE_NO_WINDOW)
    try:
     started=time.monotonic();s=None
     while time.monotonic()-started<20:
      try:s=socket.create_connection(('127.0.0.1',port),timeout=1);break
      except OSError:time.sleep(.2)
     if not s:raise TimeoutError('RDP not ready')
     s.settimeout(15);greeting=read_packet(s);root_reply=call(s,'root','getRoot');actor=root_reply.get('addonsActor')
     if not actor:raise RuntimeError('No addons actor: '+str(list(root_reply)))
     installed=call(s,actor,'installTemporaryAddon',addonPath=str(addon),openDevTools=False)
     if installed.get('error'):raise RuntimeError('Temporary test extension rejected: '+json.dumps(installed))
     sample=[reports.get(timeout=20),reports.get(timeout=20)];sample.sort(key=lambda r:r['frame'])
     assert [item['frame'] for item in sample]==[False,True], 'Expected distinct main and frame reports'
     for item in sample:item['canvasHash']=hashlib.sha256(item.pop('canvas').encode()).hexdigest();item['pixelHash']=item.pop('pixels')
     result.append(dict(run=len(result)+1,signals=sample));print(json.dumps(result[-1]),flush=True);s.close()
    finally:
     subprocess.run(['taskkill','/PID',str(process.pid),'/T','/F'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=subprocess.CREATE_NO_WINDOW)
     process.wait(timeout=15);time.sleep(1)
  assert all(v['emptyFrameProtected'] and v['emptyPixels']==v['pixelHash'] for run in result for v in run['signals']), 'synchronous empty-frame protection missing'
  assert all(v['marker'] and not v['webdriver'] for run in result for v in run['signals'])
  (root/'.qa/firefox-extension-native-empty-frame.json').write_text(json.dumps(result,indent=2))
  assert [r['canvasHash'] for r in result[0]['signals']]==[r['canvasHash'] for r in result[1]['signals']]
  assert [r['pixelHash'] for r in result[0]['signals']]==[r['pixelHash'] for r in result[1]['signals']]
  assert [r['canvasHash'] for r in result[1]['signals']]!=[r['canvasHash'] for r in result[2]['signals']]
  (root/'.qa/firefox-extension-native-empty-frame.json').write_text(json.dumps(result,indent=2))
finally:
 server.shutdown()
 registration.__exit__(None,None,None)
 manifest_root.cleanup()


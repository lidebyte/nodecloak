//! Keep a delayed Worker off the shared CDP receive loop. A first-script
//! breakpoint stays armed until every requested override and preload succeeds.
use super::*;

#[derive(Clone, Copy, PartialEq)]
enum Phase { Configuring, Installing, Removing, Resuming, Disabling, Ready }
pub(super) struct WorkerSetup {
    pending: HashMap<u64, &'static str>,
    script: Option<String>,
    breakpoint: Option<String>,
    frame: Option<String>,
    phase: Phase,
    resume: bool,
    started: Instant,
    delayed: bool,
    error: Option<String>,
    deferred: Option<Vec<(&'static str, Value)>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    fn fixture(reject: Option<&'static str>, hold_enable: bool) -> (Protocol, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            let mut held = None;
            let mut methods = vec![];
            while let Ok(Message::Text(raw)) = ws.read() {
                let command: Value = serde_json::from_str(&raw).unwrap();
                let method = command["method"].as_str().unwrap();
                methods.push(method.into());
                if hold_enable && method == "Debugger.enable" { held = Some(command["id"].clone());continue; }
                if method == "Target.getTargets" {
                    if let Some(id) = held.take() { ws.send(Message::Text(json!({"id":id,"result":{}}).to_string().into())).unwrap(); }
                }
                let reply = if Some(method) == reject {
                    json!({"id":command["id"],"error":{"code":-32000,"message":"private diagnostic must not be logged"}})
                } else {
                    json!({"id":command["id"],"result":if method == "Debugger.setInstrumentationBreakpoint" { json!({"breakpointId":"first-script"}) } else { json!({}) }})
                };
                if ws.send(Message::Text(reply.to_string().into())).is_err() { break; }
                if method == "Runtime.runIfWaitingForDebugger" {
                    ws.send(Message::Text(json!({"method":"Debugger.paused","sessionId":"worker","params":{"reason":"instrumentation","callFrames":[{"callFrameId":"first-frame"}]}}).to_string().into())).unwrap();
                }
            }
            methods
        });
        let socket = recovery::connect_socket(&format!("ws://{address}")).unwrap();
        (Protocol { worker_setups:HashMap::new(),document_setups:HashMap::new(),barrier_owner:None,socket,next:0,events:VecDeque::new(),last_error:None,navigation:None,reconnect_endpoint:None }, server)
    }
    fn pump(protocol: &mut Protocol) {
        let until = Instant::now() + Duration::from_secs(2);
        while protocol.worker_setups["worker"].phase != Phase::Ready && protocol.worker_setups["worker"].error.is_none() {
            assert!(Instant::now() < until, "Worker progress stopped");
            protocol.read().unwrap();
        }
    }
    #[test]
    fn worker_rejection_retains_first_script_pause_and_sanitizes_error() {
        for rejected in ["Debugger.enable", "Debugger.setInstrumentationBreakpoint", "Emulation.setTimezoneOverride", "Debugger.evaluateOnCallFrame"] {
            let (mut protocol, server) = fixture(Some(rejected), false);
            protocol.configure_worker("worker", vec![("Emulation.setTimezoneOverride",json!({"timezoneId":"Asia/Tokyo"}))], Some("/* fixture preload */".into()), true).unwrap();
            pump(&mut protocol);
            let error = protocol.worker_setups["worker"].error.clone().unwrap();
            assert!(error.contains(rejected));assert!(!error.contains("private diagnostic"));
            drop(protocol);
            let commands = server.join().unwrap();
            if rejected != "Debugger.evaluateOnCallFrame" {
                assert!(!commands.iter().any(|m|m == "Runtime.runIfWaitingForDebugger"), "{rejected}: unacknowledged protection released startup");
            }
            assert!(!commands.iter().any(|m|m == "Debugger.resume" || m == "Debugger.disable"), "{rejected}: rejected preload resumed first script");
        }
    }
    #[test]
    fn delayed_worker_recovers_while_a_page_command_is_waiting() {
        let (mut protocol, server) = fixture(None, true);
        protocol.configure_worker("worker", vec![("Emulation.setTimezoneOverride",json!({"timezoneId":"Asia/Tokyo"}))], Some("/* fixture preload */".into()), true).unwrap();
        protocol.worker_setups.get_mut("worker").unwrap().started = Instant::now() - Duration::from_secs(20);
        protocol.check_worker_delays();protocol.check_worker_delays();
        assert_eq!(protocol.events.iter().filter(|e|e["method"] == "NodeCloak.workerSetupDelayed").count(),1);
        protocol.call("Target.getTargets", json!({}), None).unwrap();
        pump(&mut protocol);
        assert!(protocol.events.iter().any(|e|e["method"] == "NodeCloak.workerSetupReady"));
        drop(protocol);
        let commands = server.join().unwrap();
        let index = |name:&str|commands.iter().position(|m|m == name).unwrap();
        assert!(index("Emulation.setTimezoneOverride") < index("Runtime.runIfWaitingForDebugger"));
        assert!(index("Debugger.evaluateOnCallFrame") < index("Debugger.resume"));
        assert!(index("Debugger.resume") < index("Debugger.disable"));
    }
    #[test]
    fn reattached_live_worker_does_not_wait_for_a_first_script_that_already_ran() {
        let (mut protocol, server) = fixture(None, false);
        protocol.configure_worker("worker", vec![("Emulation.setTimezoneOverride",json!({"timezoneId":"Asia/Tokyo"}))], Some("/* idempotent preload */".into()), false).unwrap();
        pump(&mut protocol);
        drop(protocol);
        let commands = server.join().unwrap();
        assert!(commands.iter().any(|m|m == "Runtime.evaluate"));
        assert!(!commands.iter().any(|m|m.starts_with("Debugger.") || m == "Runtime.runIfWaitingForDebugger"));
    }
}
impl Protocol {
    pub(super) fn configure_worker(&mut self, session: &str, commands: Vec<(&'static str, Value)>, script: Option<String>, waiting: bool) -> Result<(), String> {
        if let Some(setup) = self.worker_setups.get_mut(session) {
            if setup.error.is_some() { return Ok(()); }
            if setup.frame.is_some() || matches!(setup.phase, Phase::Installing | Phase::Removing | Phase::Resuming | Phase::Disabling) {
                setup.deferred = Some(commands);
                return Ok(());
            }
            if setup.phase == Phase::Ready {
                setup.phase = Phase::Configuring;setup.started = Instant::now();setup.delayed = false;
            }
        } else {
            self.worker_setups.insert(session.into(), WorkerSetup {
                pending:HashMap::new(), script:if waiting { script.clone() } else { None },
                breakpoint:None, frame:None, phase:Phase::Configuring, resume:waiting,
                started:Instant::now(), delayed:false, error:None, deferred:None,
            });
            if script.is_some() && waiting {
                self.worker_command(session, "Debugger.enable", json!({}))?;
                self.worker_command(session, "Debugger.setInstrumentationBreakpoint", json!({"instrumentation":"beforeScriptExecution"}))?;
            }
        }
        for (method, params) in commands { self.worker_command(session, method, params)?; }
        // Reattached Workers are already running and will never hit a first-script
        // breakpoint. Reapply the same idempotent preload in their current realm.
        if !waiting {
            if let Some(script) = script {
                self.worker_command(session, "Runtime.evaluate", json!({"expression":script,"silent":true,"returnByValue":true}))?;
            }
        }
        self.advance_worker(session)
    }
    fn worker_command(&mut self, session: &str, method: &'static str, params: Value) -> Result<(), String> {
        self.next += 1;
        self.worker_setups.get_mut(session).unwrap().pending.insert(self.next, method);
        self.socket.send(Message::Text(json!({"id":self.next,"method":method,"params":params,"sessionId":session}).to_string().into()))
            .map_err(|e|format!("无法写入后台任务设置 {method}（{}）",recovery::transport_error(&e)))
    }
    fn fail_worker(&mut self, session: &str, error: String) {
        let setup = self.worker_setups.get_mut(session).unwrap();
        if setup.error.is_none() {
            setup.error = Some(error.clone());
            self.events.push_back(json!({"method":"NodeCloak.workerSetupFailed","sessionId":session,"params":{"error":error}}));
        }
    }
    fn advance_worker(&mut self, session: &str) -> Result<(), String> {
        let setup = &self.worker_setups[session];
        if setup.error.is_some() || !setup.pending.is_empty() || setup.phase == Phase::Ready { return Ok(()); }
        match setup.phase {
            Phase::Configuring => {
                // A queued breakpoint is not an acknowledged breakpoint. Keep
                // startup paused until all protection settings have succeeded.
                if setup.resume {
                    self.worker_setups.get_mut(session).unwrap().resume = false;
                    self.worker_command(session, "Runtime.runIfWaitingForDebugger", json!({}))?;
                    return Ok(());
                }
                if let Some(script) = &setup.script {
                    let Some(frame) = &setup.frame else { return Ok(()); };
                    let params = json!({"callFrameId":frame,"expression":script,"returnByValue":true,"silent":true});
                    self.worker_setups.get_mut(session).unwrap().phase = Phase::Installing;
                    self.worker_command(session, "Debugger.evaluateOnCallFrame", params)?;
                    return Ok(());
                }
            }
            Phase::Installing => {
                let breakpoint = setup.breakpoint.clone().ok_or("后台任务保护断点无效")?;
                self.worker_setups.get_mut(session).unwrap().phase = Phase::Removing;
                self.worker_command(session, "Debugger.removeBreakpoint", json!({"breakpointId":breakpoint}))?;
                return Ok(());
            }
            Phase::Removing => {
                self.worker_setups.get_mut(session).unwrap().phase = Phase::Resuming;
                self.worker_command(session, "Debugger.resume", json!({}))?;
                return Ok(());
            }
            Phase::Resuming => {
                self.worker_setups.get_mut(session).unwrap().phase = Phase::Disabling;
                self.worker_command(session, "Debugger.disable", json!({}))?;
                return Ok(());
            }
            Phase::Disabling => {},
            Phase::Ready => unreachable!(),
        }
        let setup = self.worker_setups.get_mut(session).unwrap();
        setup.phase = Phase::Ready;
        setup.script = None;
        setup.frame = None;
        if let Some(commands) = setup.deferred.take() {
            let delayed = setup.delayed;
            self.configure_worker(session, commands, None, false)?;
            self.worker_setups.get_mut(session).unwrap().delayed |= delayed;
        } else if setup.delayed {
            self.events.push_back(json!({"method":"NodeCloak.workerSetupReady","sessionId":session}));
        }
        Ok(())
    }
    pub(super) fn route_worker_reply(&mut self, value: &Value) -> Result<bool, String> {
        if value["method"] == "Debugger.paused" {
            if let Some(session) = value["sessionId"].as_str().filter(|s|self.worker_setups.get(*s).is_some_and(|w|w.script.is_some())) {
                if value["params"]["reason"] == "instrumentation" {
                    if let Some(frame) = value["params"]["callFrames"][0]["callFrameId"].as_str() {
                        self.worker_setups.get_mut(session).unwrap().frame = Some(frame.into());
                        self.advance_worker(session)?;
                    } else { self.fail_worker(session, "后台任务指纹初始化未获得执行上下文".into()); }
                    return Ok(true);
                }
            }
        }
        let Some(id) = value["id"].as_u64() else { return Ok(false); };
        let Some(session) = self.worker_setups.iter().find_map(|(s,w)|w.pending.contains_key(&id).then(||s.clone())) else { return Ok(false); };
        let method = self.worker_setups.get_mut(&session).unwrap().pending.remove(&id).unwrap();
        if let Some(error) = value.get("error") {
            let message = error["message"].as_str().unwrap_or("");
            let inherited = (method == "Emulation.setLocaleOverride" && message == "Another locale override is already in effect")
                || (method == "Emulation.setTimezoneOverride" && message.trim_end_matches('.') == "Timezone override is already in effect");
            if !inherited {
                self.fail_worker(&session, format!("浏览器拒绝后台任务设置 {method}（code={}）",error["code"].as_i64().map(|n|n.to_string()).unwrap_or_else(||"unknown".into())));
            }
        } else if value["result"].get("exceptionDetails").is_some() {
            self.fail_worker(&session, format!("后台任务设置执行失败：{method}"));
        } else if method == "Debugger.setInstrumentationBreakpoint" {
            self.worker_setups.get_mut(&session).unwrap().breakpoint = value["result"]["breakpointId"].as_str().map(str::to_owned);
            if self.worker_setups[&session].breakpoint.is_none() { self.fail_worker(&session, "后台任务保护断点无效".into()); }
        }
        self.advance_worker(&session)?;
        Ok(true)
    }
    pub(super) fn check_worker_delays(&mut self) {
        for (session, setup) in &mut self.worker_setups {
            if setup.phase != Phase::Ready && setup.error.is_none() && !setup.delayed && setup.started.elapsed() >= Duration::from_secs(15) {
                setup.delayed = true;
                let mut pending:Vec<_> = setup.pending.values().copied().collect();pending.sort_unstable();
                let phase = match setup.phase {
                    Phase::Configuring if setup.pending.is_empty() && setup.script.is_some()=>"waiting_for_first_script",
                    Phase::Configuring=>"configuring",Phase::Installing=>"installing_preload",Phase::Removing=>"removing_breakpoint",
                    Phase::Resuming=>"resuming",Phase::Disabling=>"disabling_debugger",Phase::Ready=>"ready",
                };
                self.events.push_back(json!({"method":"NodeCloak.workerSetupDelayed","sessionId":session,"params":{"pending":pending,"phase":phase}}));
            }
        }
    }
}

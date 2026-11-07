//! Native smoke check, no bot core, OAuth credentials, tray, OBS or user profile.
use signorebot_lib::twitch_view::{update, NativeChat};
use tauri::Manager;
#[tauri::command]
fn probe_private() -> &'static str {
    "REMOTE_IPC_MUST_NOT_REACH_THIS"
}
fn main() {
    let root = std::sync::Arc::new(tempfile::tempdir().expect("temporary Twitch profile"));
    tauri::Builder::default().invoke_handler(tauri::generate_handler![probe_private]).setup(move |app| {
        let handle = app.handle().clone();
        let root = std::sync::Arc::clone(&root);
        // Keep the profile directory alive until the process exits.
        let native = NativeChat::ephemeral();
        if let Some(w) = app.get_webview_window("main") { w.set_title("SignoreBot Twitch Popout Probe")?; w.navigate("about:blank".parse()?)?; }
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let bounds = || serde_json::from_value(serde_json::json!({"x":20,"y":80,"width":600,"height":600})).unwrap();
            let result = update(&handle,&native,root.path(),Some("twitch".into()),Some(bounds()),false);
            println!("CREATE: {result:?}");
            if result.is_err() { handle.exit(1); return; }
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            if let Some(w) = handle.get_webview("twitch-chat") {
                println!("CHILD URL: {:?}; SIZE: {:?}", w.url(), w.size());
                #[cfg(target_os = "linux")]
                {
                    use gtk::prelude::*;
                    let (tx, rx) = std::sync::mpsc::sync_channel(1);
                    w.with_webview(move |p| { let a=p.inner().allocation(); let _=tx.send((a.width(),a.height(),p.inner().margin_start(),p.inner().margin_top())); }).unwrap();
                    let geometry=rx.recv().unwrap(); println!("GTK CLIENT: {geometry:?}");
                    if geometry != (600,600,20,80) { println!("FAIL: incorrect child bounds"); handle.exit(1); return; }
                }
                #[cfg(not(target_os = "linux"))]
                if w.size().map(|s| s.width != 600 || s.height != 600).unwrap_or(true) { handle.exit(1); return; }
                let (tx, rx) = std::sync::mpsc::sync_channel(1);
                w.eval_with_callback("JSON.stringify({title:document.title,text:document.body.innerText.slice(0,100),width:innerWidth,height:innerHeight})", move |value| { let _=tx.send(value); }).unwrap();
                println!("DOCUMENT: {:?}",rx.recv_timeout(std::time::Duration::from_secs(3)));
                w.eval("window.__probeIPC='unavailable';if(window.__TAURI_INTERNALS__?.invoke){window.__probeIPC='pending';window.__TAURI_INTERNALS__.invoke('probe_private').then(v=>window.__probeIPC=v).catch(e=>window.__probeIPC=String(e))}").unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let (tx, rx) = std::sync::mpsc::sync_channel(1);
                w.eval_with_callback("window.__probeIPC", move |value| { let _=tx.send(value); }).unwrap();
                let ipc=rx.recv_timeout(std::time::Duration::from_secs(3)).unwrap_or_default(); println!("REMOTE IPC: {ipc}");
                if ipc.contains("REMOTE_IPC_MUST_NOT_REACH_THIS") || ipc.contains("pending") || ipc.is_empty() { handle.exit(1); return; }
                w.eval("window.open('https://www.twitch.tv/login','signorebot-probe-login')").unwrap();
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let popups: Vec<_> = handle.webview_windows().into_iter().filter(|(label,_)|label.starts_with("twitch-login-")).collect();
                println!("LOGIN POPUPS: {}", popups.len());
                for (_, popup) in popups { println!("LOGIN URL: {:?}",popup.url()); let _=popup.close(); }
            }
            println!("HIDE: {:?}",update(&handle,&native,root.path(),None,None,false));
            println!("SHOW: {:?}",update(&handle,&native,root.path(),Some("twitch".into()),Some(bounds()),false));
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            println!("RELOAD: {:?}",update(&handle,&native,root.path(),Some("twitch".into()),Some(bounds()),true));
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            handle.exit(0);
        });
        Ok(())
    }).run(tauri::generate_context!()).expect("native probe");
}

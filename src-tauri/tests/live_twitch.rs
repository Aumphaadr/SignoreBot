//! Живой Twitch (ignored): cargo test --test live_twitch -- --ignored --nocapture
use signorebot_lib::twitch::auth;

#[tokio::test]
#[ignore]
async fn device_code_live() {
    let r = auth::device_code(signorebot_lib::config::DEFAULT_TWITCH_CLIENT_ID, auth::BROADCASTER_SCOPES).await;
    match r {
        Ok(dc) => println!("OK user_code={} uri={}", dc.user_code, dc.verification_uri),
        Err(e) => panic!("ERR {e:?}"),
    }
}

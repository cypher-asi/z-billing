//! Static checkout result pages served directly by z-billing.
//!
//! These are shown after Stripe Checkout redirects back.

use axum::response::Html;

const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Subscription Successful - AURA</title>
  <style>
    @import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600&display=swap');
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: 'Inter', sans-serif;
      background: #0d0d0d;
      color: #e0e0e0;
      min-height: 100vh;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
    }
    .container { text-align: center; max-width: 420px; padding: 0 24px; }
    .icon {
      width: 64px; height: 64px; border-radius: 50%;
      background: rgba(1, 244, 203, 0.1); border: 2px solid #01f4cb;
      display: flex; align-items: center; justify-content: center;
      margin: 0 auto 32px;
    }
    .icon svg { width: 32px; height: 32px; stroke: #01f4cb; fill: none; stroke-width: 2.5; stroke-linecap: round; stroke-linejoin: round; }
    h1 { font-size: 24px; font-weight: 600; margin-bottom: 12px; color: #fff; }
    p { font-size: 15px; color: #a0a0a0; line-height: 1.6; }
    .brand { position: fixed; top: 32px; left: 32px; font-size: 14px; font-weight: 600; letter-spacing: 0.1em; color: #e0e0e0; }
  </style>
</head>
<body>
  <div class="brand">AURA</div>
  <div class="container">
    <div class="icon">
      <svg viewBox="0 0 24 24"><polyline points="20 6 9 17 4 12"></polyline></svg>
    </div>
    <h1>Subscription Successful</h1>
    <p>Your plan has been activated and credits have been added to your account. You can close this page and return to AURA.</p>
  </div>
</body>
</html>"#;

const CANCELLED_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Checkout Cancelled - AURA</title>
  <style>
    @import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600&display=swap');
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: 'Inter', sans-serif;
      background: #0d0d0d;
      color: #e0e0e0;
      min-height: 100vh;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
    }
    .container { text-align: center; max-width: 420px; padding: 0 24px; }
    .icon {
      width: 64px; height: 64px; border-radius: 50%;
      background: rgba(160, 160, 160, 0.1); border: 2px solid #606060;
      display: flex; align-items: center; justify-content: center;
      margin: 0 auto 32px;
    }
    .icon svg { width: 32px; height: 32px; stroke: #a0a0a0; fill: none; stroke-width: 2.5; stroke-linecap: round; stroke-linejoin: round; }
    h1 { font-size: 24px; font-weight: 600; margin-bottom: 12px; color: #fff; }
    p { font-size: 15px; color: #a0a0a0; line-height: 1.6; }
    .brand { position: fixed; top: 32px; left: 32px; font-size: 14px; font-weight: 600; letter-spacing: 0.1em; color: #e0e0e0; }
  </style>
</head>
<body>
  <div class="brand">AURA</div>
  <div class="container">
    <div class="icon">
      <svg viewBox="0 0 24 24"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
    </div>
    <h1>Checkout Cancelled</h1>
    <p>Your subscription was not completed. You can close this page and return to AURA to try again.</p>
  </div>
</body>
</html>"#;

/// Checkout success page.
pub async fn success() -> Html<&'static str> {
    Html(SUCCESS_HTML)
}

/// Checkout cancelled page.
pub async fn cancelled() -> Html<&'static str> {
    Html(CANCELLED_HTML)
}

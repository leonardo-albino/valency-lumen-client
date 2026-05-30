// Valency Lumen — heartbeat HTTP pro Hub a cada 30s.
//
// Por que: o `peer.created_at` no SQLite do hbbs (fonte original do
// `ultimo_visto_em` no Hub) so e setado no 1o handshake. Reconexoes nao
// atualizam, entao clientes ativos aparecem como "offline" no PresenceTracker.
//
// O que faz: POST sem body em
//   https://hub.vn.net.br/api/v1/lumen/devices/<peer_id>/heartbeat
// a cada 30s, em uma thread dedicada (nao Tokio — evita acoplar com o runtime
// do servidor e segue as regras do AGENTS.md). Resposta esperada: 204.
//
// Falhas (network, DNS, 5xx) sao silenciosas — heartbeat nao pode quebrar o
// app cliente nem floodar o log. So roda em Windows (target real do Lumen).

#[cfg(target_os = "windows")]
pub fn spawn_heartbeat() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        // Idempotente: se chamado mais de uma vez (ex.: por reentry de
        // start_server), so spawna uma thread.
        return;
    }

    std::thread::Builder::new()
        .name("lumen-heartbeat".to_owned())
        .spawn(|| {
            // Url base via env-var compile-time (workflow injeta LUMEN_API_SERVER).
            // Fallback hard-coded pro prod evita config errada em build local.
            let base = option_env!("LUMEN_API_SERVER")
                .unwrap_or("https://hub.vn.net.br")
                .trim_end_matches('/')
                .to_owned();

            // Reqwest blocking client compartilhado (ja em Cargo.toml com feature
            // "blocking"). Timeout curto: nao queremos travar a thread se o Hub
            // estiver lento.
            let client = match reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
            {
                Ok(c) => c,
                Err(_) => return, // builder so falha em config invalida; aborta silente
            };

            loop {
                let peer_id = hbb_common::config::Config::get_id();
                if peer_id.is_empty() {
                    // Antes do 1o gen_id() pode estar vazio — espera 5s e tenta de novo
                    // sem comer 30s do ciclo.
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    continue;
                }

                let url = format!("{}/api/v1/lumen/devices/{}/heartbeat", base, peer_id);
                // Ignora o resultado: erros de rede/server nao devem poluir nada.
                let _ = client.post(&url).send();

                std::thread::sleep(std::time::Duration::from_secs(30));
            }
        })
        .ok();
}

#[cfg(not(target_os = "windows"))]
pub fn spawn_heartbeat() {}

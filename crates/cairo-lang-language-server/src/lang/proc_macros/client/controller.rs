use std::sync::{Arc, Mutex};

use cairo_lang_defs::db::DefsGroup;
use proc_macro_server_api::methods::defined_macros::DefinedMacrosResponse;

use super::ProcMacroClient;
use crate::config::Config;
use crate::lang::db::AnalysisDatabase;
use crate::lang::proc_macros::cache_group::ProcMacroCacheGroup;
use crate::lang::proc_macros::connection::ProcMacroServerConnection;
use crate::lang::proc_macros::idle_job::apply_proc_macro_server_responses;
use crate::lang::proc_macros::plugins::proc_macro_plugin_suite;
use crate::toolchain::scarb::ScarbToolchain;
use tracing::error;

/// Manages lifecycle of proc-macro-server client.
pub struct ProcMacroClientController {
    status_change: ProcMacroClientStatusChange,
    retries_left: u8,
}

#[derive(Debug, Clone, Default)]
pub struct ProcMacroClientStatusChange(Arc<Mutex<Option<ClientStatusChange>>>);

impl ProcMacroClientStatusChange {
    pub fn update(&self, change: ClientStatusChange) {
        *self.0.lock().unwrap() = Some(change);
    }

    pub fn changed(&self) -> Option<ClientStatusChange> {
        self.0.lock().unwrap().take()
    }
}

impl ProcMacroClientController {
    pub fn new() -> Self {
        Self { status_change: Default::default(), retries_left: 3 }
    }

    fn should_initialize(&mut self) -> bool {
        // TODO probably something better can be used here like 3 times in 5 minutes or something.
        self.retries_left -= 1;

        self.retries_left != 0
    }

    /// Runs in post sync task hook. Commonly starts proc-macro-server after config reload.
    pub fn initialize_if_enabled_now(
        &mut self,
        db: &mut AnalysisDatabase,
        config: &Config,
        scarb: &ScarbToolchain,
    ) {
        if let ClientStatus::Disabled = db.proc_macro_client_status() {
            self.initialize(db, config, scarb);
        }
    }

    /// Adds proc-macro-server related functionalities to db, if enabled.
    pub fn initialize(
        &mut self,
        db: &mut AnalysisDatabase,
        config: &Config,
        scarb: &ScarbToolchain,
    ) {
        if config.disable_proc_macros {
            return;
        }

        match scarb.proc_macro_server() {
            Ok(proc_macro_server) => {
                let client = Arc::new(ProcMacroClient::new(
                    ProcMacroServerConnection::new(proc_macro_server),
                    self.status_change.clone(),
                ));

                db.set_proc_macro_client_status(ClientStatus::Initializing);

                std::thread::spawn({
                    let status_change = self.status_change.clone();

                    move || match client.fetch_defined_macros() {
                        Ok(defined_macros) => {
                            status_change.update(ClientStatusChange::Ready(defined_macros, client));
                        }
                        Err(err) => {
                            error!("Fetching defined macros failed: {err:?}");

                            status_change.update(ClientStatusChange::Failed);
                        }
                    }
                });
            }
            Err(_) => {
                self.status_change.update(ClientStatusChange::FatalFailed);
            }
        }
    }

    pub fn maybe_update_state(
        &mut self,
        db: &mut AnalysisDatabase,
        config: &Config,
        scarb: &ScarbToolchain,
    ) -> bool {
        let mut result = false;
        if let Some(status) = self.status_change.changed() {
            self.update_state(db, config, scarb, status);

            result = true;
        };

        apply_proc_macro_server_responses(db) || result
    }

    fn update_state(
        &mut self,
        db: &mut AnalysisDatabase,
        config: &Config,
        scarb: &ScarbToolchain,
        status: ClientStatusChange,
    ) {
        match status {
            ClientStatusChange::Failed => {
                if self.should_initialize() {
                    self.initialize(db, config, scarb);
                } else {
                    db.set_proc_macro_client_status(ClientStatus::InitializingFailed);
                }
            }
            ClientStatusChange::FatalFailed => {
                db.set_proc_macro_client_status(ClientStatus::InitializingFailed);
                // TODO notify
            }
            ClientStatusChange::Ready(defined_macros, client) => {
                let plugin = proc_macro_plugin_suite(defined_macros);
                // TODO we should remove previous proc-macro-server instane plugins here but no idea how to do this.

                let mut macro_plugins = db.macro_plugins();
                macro_plugins.extend(plugin.plugins);
                db.set_macro_plugins(macro_plugins);

                let mut inline_macro_plugins = Arc::unwrap_or_clone(db.inline_macro_plugins());
                inline_macro_plugins.extend(plugin.inline_macro_plugins);
                db.set_inline_macro_plugins(inline_macro_plugins.clone().into());

                db.set_proc_macro_client_status(ClientStatus::Ready(client));
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ClientStatus {
    Disabled,
    Initializing,
    Ready(Arc<ProcMacroClient>),
    // After retries it does not work. No more actions will be done.
    InitializingFailed,
}

impl ClientStatus {
    pub fn ready(&self) -> Option<&ProcMacroClient> {
        if let Self::Ready(client) = self {
            Some(client)
        } else {
            None
        }
    }
}

/// Edges of [`ClientStatus`].
/// Represents possible state transitions.
#[derive(Debug)]
pub enum ClientStatusChange {
    Ready(DefinedMacrosResponse, Arc<ProcMacroClient>),
    // We can retry.
    Failed,
    // Even if we retry it probably won't work anyway.
    FatalFailed,
}

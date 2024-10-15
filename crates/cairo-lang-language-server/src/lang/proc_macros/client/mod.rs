use std::sync::Mutex;

use anyhow::{Context, Result, anyhow, bail};
use controller::{ClientStatusChange, ProcMacroClientStatusChange};
use proc_macro_server_api::methods::defined_macros::{
    DefinedMacros, DefinedMacrosParams, DefinedMacrosResponse,
};
use proc_macro_server_api::methods::expand::{
    ExpandAttribute, ExpandAttributeParams, ExpandDerive, ExpandDeriveParams, ExpandInline,
    ExpandInlineMacroParams,
};
use proc_macro_server_api::{Method, RequestId, RpcRequest, RpcResponse};
use rustc_hash::FxHashMap;
use tracing::error;

use super::connection::ProcMacroServerConnection;
use super::id_generator::IdGenerator;

pub mod controller;

#[derive(Debug)]
pub enum RequestParams {
    Attribute(ExpandAttributeParams),
    Derive(ExpandDeriveParams),
    Inline(ExpandInlineMacroParams),
}

#[derive(Debug)]
pub struct ProcMacroClient {
    connection: ProcMacroServerConnection,
    status_change: ProcMacroClientStatusChange,
    id_generator: IdGenerator,
    pub(super) requests_params: Mutex<FxHashMap<RequestId, RequestParams>>,
}

impl ProcMacroClient {
    pub fn new(
        connection: ProcMacroServerConnection,
        status_change: ProcMacroClientStatusChange,
    ) -> Self {
        Self {
            connection,
            status_change,
            id_generator: Default::default(),
            requests_params: Default::default(),
        }
    }

    /// Reads all available responses without waiting for new ones.
    pub fn available_responses(&self) -> impl Iterator<Item = RpcResponse> + '_ {
        self.connection.responder.try_iter()
    }
}

/// Used by [`ProcMacroCacheGroup`]
impl ProcMacroClient {
    pub fn request_attribute(&self, params: ExpandAttributeParams) {
        self.send_request_tracked::<ExpandAttribute>(params, RequestParams::Attribute)
    }

    pub fn request_derives(&self, params: ExpandDeriveParams) {
        self.send_request_tracked::<ExpandDerive>(params, RequestParams::Derive)
    }

    pub fn request_inline_macros(&self, params: ExpandInlineMacroParams) {
        self.send_request_tracked::<ExpandInline>(params, RequestParams::Inline)
    }

    fn fetch_defined_macros(&self) -> Result<DefinedMacrosResponse> {
        let id = self.send_request::<DefinedMacros>(&DefinedMacrosParams {})?;

        if id == 0 {
            bail!(
                "fetching defined macros should be first sended request, it is {id} (zero \
                 counting)"
            )
        }

        // This works because this it is first request we sends and we wait for response before
        // sending any more requests.
        let response = self
            .connection
            .responder
            .recv()
            .context("failed to read response for defined macros request")?;

        if response.id != id {
            bail!(
                "fetching defined macros should be waited before any other request is send, \
                 received response for {id} <- should be 0"
            )
        }

        serde_json::from_value(response.value)
            .context("failed to deserialize response for defined macros request")
    }

    fn send_request<M: Method>(&self, params: &M::Params) -> Result<RequestId> {
        let id = self.id_generator.unique_id();

        self.connection
            .requester
            .send(RpcRequest {
                id,
                method: M::METHOD.to_string(),
                value: serde_json::to_value(params).unwrap(),
            })
            .with_context(|| anyhow!("sending request {id} failed"))
            .map(|_| id)
    }

    fn send_request_tracked<M: Method>(
        &self,
        params: M::Params,
        map: impl FnOnce(M::Params) -> RequestParams,
    ) {
        match self.send_request::<M>(&params) {
            Ok(id) => {
                self.requests_params.lock().unwrap().insert(id, map(params));
            }
            Err(err) => {
                error!("Sending request to proc-macro-server failed: {err:?}");

                self.status_change.update(ClientStatusChange::Failed);
            }
        }
    }
}

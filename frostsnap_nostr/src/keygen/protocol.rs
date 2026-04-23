use crate::{
    channel::ChannelKeys,
    channel_runner::{
        decode_bincode, ChannelMessageDraft, ChannelRunner, ChannelRunnerEvent, ChannelRunnerHandle,
    },
};
use anyhow::Result;
use frostsnap_coordinator::Sink;
use frostsnap_core::{
    coordinator::remote_keygen::{RemoteKeygenMessage, RemoteKeygenPayload},
    DeviceId,
};
use nostr_sdk::{Client, EventId, Keys, Kind, PublicKey};
use std::collections::BTreeMap;

pub const KIND_FROSTSNAP_KEYGEN_PROTOCOL: Kind = Kind::Custom(9002);

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct KeygenProtocolMessage {
    pub from: DeviceId,
    pub payload: RemoteKeygenPayload,
}

pub struct ProtocolClient;

impl ProtocolClient {
    /// Start a dedicated protocol channel runner for a keygen session.
    ///
    /// `allowed_senders` maps each coordinator's nostr pubkey to the set of
    /// `DeviceId`s it is authorized to send messages as. Any inner event
    /// signed by a pubkey outside this map — or claiming a `from` not owned by
    /// its signer — is dropped.
    pub async fn run(
        client: Client,
        channel_keys: ChannelKeys,
        keygen_event_id: EventId,
        allowed_senders: BTreeMap<PublicKey, Vec<DeviceId>>,
        sink: impl Sink<RemoteKeygenMessage> + Clone + Sync,
    ) -> Result<ProtocolHandle> {
        let (runner_handle, mut events) = ChannelRunner::new(channel_keys)
            .with_message_expiration(crate::keygen::KEYGEN_MESSAGE_TTL)
            .run(client)
            .await?;
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                let ChannelRunnerEvent::AppEvent { inner_event } = event else {
                    continue;
                };
                if inner_event.kind != KIND_FROSTSNAP_KEYGEN_PROTOCOL {
                    continue;
                }
                let msg: KeygenProtocolMessage = match decode_bincode(&inner_event) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!(
                            event_id = %inner_event.id,
                            error = %e,
                            "failed to decode keygen protocol message"
                        );
                        continue;
                    }
                };
                let Some(allowed) = allowed_senders.get(&inner_event.pubkey) else {
                    tracing::warn!(
                        event_id = %inner_event.id,
                        signer = %inner_event.pubkey,
                        "keygen protocol message signed by non-participant, dropping"
                    );
                    continue;
                };
                if !allowed.contains(&msg.from) {
                    tracing::warn!(
                        event_id = %inner_event.id,
                        signer = %inner_event.pubkey,
                        claimed_from = %msg.from,
                        "keygen protocol message 'from' not owned by signer, dropping"
                    );
                    continue;
                }
                sink.send(RemoteKeygenMessage {
                    from: msg.from,
                    payload: msg.payload,
                });
            }
        });
        Ok(ProtocolHandle {
            runner_handle,
            keygen_event_id,
        })
    }
}

#[derive(Clone)]
pub struct ProtocolHandle {
    runner_handle: ChannelRunnerHandle,
    keygen_event_id: EventId,
}

impl ProtocolHandle {
    pub async fn send_keygen_payload(
        &self,
        keys: &Keys,
        from: DeviceId,
        payload: RemoteKeygenPayload,
    ) -> Result<EventId> {
        let msg = KeygenProtocolMessage { from, payload };
        let draft = ChannelMessageDraft::app(
            KIND_FROSTSNAP_KEYGEN_PROTOCOL,
            &msg,
            vec![self.keygen_event_id],
        )?;
        let prepared = self.runner_handle.send(keys, draft).await?;
        Ok(prepared.id)
    }
}

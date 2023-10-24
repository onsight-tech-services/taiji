// Copyright 2023, OnSight Tech Services LLC
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{convert::TryFrom, ffi::CString, ops::Deref};

use libc::c_char;
use log::{debug, error, info, trace};
use taiji_contacts::contacts_service::{
    handle::{ContactsLivenessData, ContactsLivenessEvent, ContactsServiceHandle},
    types::Message,
};
use taiji_shutdown::ShutdownSignal;

const LOG_TARGET: &str = "chat_ffi::callback_handler";

pub(crate) type CallbackContactStatusChange = unsafe extern "C" fn(*mut ChatFFIContactsLivenessData);
pub(crate) type CallbackMessageReceived = unsafe extern "C" fn(*mut ChatFFIMessage);

#[repr(C)]
pub struct ChatFFIContactsLivenessData {
    pub address: *const c_char,
    pub last_seen: u64,
    pub online_status: u8,
}

impl TryFrom<ContactsLivenessData> for ChatFFIContactsLivenessData {
    type Error = String;

    fn try_from(v: ContactsLivenessData) -> Result<Self, Self::Error> {
        let address = match CString::new(v.address().to_bytes()) {
            Ok(s) => s,
            Err(e) => return Err(e.to_string()),
        };

        let last_seen = match v.last_ping_pong_received() {
            Some(ts) => match u64::try_from(ts.timestamp_micros()) {
                Ok(num) => num,
                Err(e) => return Err(e.to_string()),
            },
            None => 0,
        };

        Ok(Self {
            address: address.as_ptr(),
            last_seen,
            online_status: v.online_status().as_u8(),
        })
    }
}

#[repr(C)]
pub struct ChatFFIMessage {
    pub body: *const c_char,
    pub from_address: *const c_char,
    pub stored_at: u64,
    pub message_id: *const c_char,
}

impl TryFrom<Message> for ChatFFIMessage {
    type Error = String;

    fn try_from(v: Message) -> Result<Self, Self::Error> {
        let body = match CString::new(v.body) {
            Ok(s) => s,
            Err(e) => return Err(e.to_string()),
        };

        let address = match CString::new(v.address.to_bytes()) {
            Ok(s) => s,
            Err(e) => return Err(e.to_string()),
        };

        let id = match CString::new(v.message_id) {
            Ok(s) => s,
            Err(e) => return Err(e.to_string()),
        };

        Ok(Self {
            body: body.as_ptr(),
            from_address: address.as_ptr(),
            stored_at: v.stored_at,
            message_id: id.as_ptr(),
        })
    }
}

#[derive(Clone)]
pub struct CallbackHandler {
    contacts_service_handle: ContactsServiceHandle,
    callback_contact_status_change: CallbackContactStatusChange,
    callback_message_received: CallbackMessageReceived,
    shutdown: ShutdownSignal,
}

impl CallbackHandler {
    pub fn new(
        contacts_service_handle: ContactsServiceHandle,
        shutdown: ShutdownSignal,
        callback_contact_status_change: CallbackContactStatusChange,
        callback_message_received: CallbackMessageReceived,
    ) -> Self {
        Self {
            contacts_service_handle,
            shutdown,
            callback_contact_status_change,
            callback_message_received,
        }
    }

    pub(crate) async fn start(&mut self) {
        let mut liveness_events = self.contacts_service_handle.get_contacts_liveness_event_stream();
        let mut chat_messages = self.contacts_service_handle.get_messages_event_stream();

        loop {
            tokio::select! {
                rec_message = chat_messages.recv() => {
                    match rec_message {
                        Ok(message) => {
                            trace!(target: LOG_TARGET, "FFI Callback monitor received a new Message");
                            self.trigger_message_received(message.deref().clone());
                        },
                        Err(_) => { debug!(target: LOG_TARGET, "FFI Callback monitor had an error receiving new messages")}
                    }
                },

                event = liveness_events.recv() => {
                    match event {
                        Ok(liveness_event) => {
                            match liveness_event.deref() {
                                ContactsLivenessEvent::StatusUpdated(data) => {
                                    trace!(target: LOG_TARGET,
                                        "FFI Callback monitor received Contact Status Updated event"
                                    );
                                    self.trigger_contact_status_change(data.deref().clone());
                                }
                                ContactsLivenessEvent::NetworkSilence => {},
                            }
                        },
                        Err(_) => { debug!(target: LOG_TARGET, "FFI Callback monitor had an error with contacts liveness")}
                    }
                },
                _ = self.shutdown.wait() => {
                    info!(target: LOG_TARGET, "ChatFFI Callback Handler shutting down because the shutdown signal was received");
                    break;
                },
            }
        }
    }

    fn trigger_contact_status_change(&mut self, data: ContactsLivenessData) {
        debug!(
            target: LOG_TARGET,
            "Calling ContactStatusChanged callback function for contact {}",
            data.address(),
        );

        match ChatFFIContactsLivenessData::try_from(data) {
            Ok(data) => unsafe {
                (self.callback_contact_status_change)(Box::into_raw(Box::new(data)));
            },
            Err(e) => {
                error!(target: LOG_TARGET, "Error processing contacts liveness data received callback: {}", e)
            },
        }
    }

    fn trigger_message_received(&mut self, message: Message) {
        debug!(
            target: LOG_TARGET,
            "Calling MessageReceived callback function for sender {}",
            message.address,
        );

        match ChatFFIMessage::try_from(message) {
            Ok(message) => unsafe {
                (self.callback_message_received)(Box::into_raw(Box::new(message)));
            },
            Err(e) => error!(target: LOG_TARGET, "Error processing message received callback: {}", e),
        }
    }
}

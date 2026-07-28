//! Session caches (chats, handles, contacts, tapbacks).

use std::collections::{BTreeSet, HashMap, HashSet};

use imessage_database::{
    tables::{
        chat::Chat,
        chat_handle::ChatToHandle,
        handle::Handle,
        messages::Message,
        table::{Cacheable, ME, UNKNOWN},
    },
    util::dates::get_offset,
};

use crate::{
    contacts::Name, data_source::DataSource, error::RuntimeError, options::MailOptions,
};

/// Bootstrap state for one mail export.
pub struct MailSession {
    pub options: MailOptions,
    pub offset: i64,
    pub data_source: DataSource,
    pub chatrooms: HashMap<i32, Chat>,
    pub real_chatrooms: HashMap<i32, i32>,
    pub chatroom_participants: HashMap<i32, BTreeSet<i32>>,
    pub participants: HashMap<i32, Name>,
    pub real_participants: HashMap<i32, i32>,
    /// Tapbacks keyed by target message GUID → part index → reactions.
    pub tapbacks: HashMap<String, HashMap<usize, Vec<Message>>>,
}

impl MailSession {
    pub fn new(options: MailOptions) -> Result<Self, RuntimeError> {
        let data_source = DataSource::from(&options)?;

        eprintln!("Building cache...");
        eprintln!("  [1/4] Caching chats...");
        let chatrooms = Chat::cache(data_source.db())?;

        eprintln!("  [2/4] Caching chatrooms...");
        let chatroom_participants = ChatToHandle::cache(data_source.db())?;
        let chat_handle_lookup = ChatToHandle::get_chat_lookup_map(data_source.db())?;
        let real_chatrooms = ChatToHandle::dedupe(&chatroom_participants, &chat_handle_lookup)?;

        eprintln!("  [3/4] Caching participants...");
        let participants = Handle::cache(data_source.db())?;
        let real_participants = Handle::dedupe(&participants);
        let participants_map = data_source
            .contacts_index
            .build_participants_map(&participants, &real_participants);

        eprintln!("  [4/4] Caching tapbacks...");
        let tapbacks = Message::cache(data_source.db())?;
        eprintln!("Cache built!");

        Ok(Self {
            chatrooms,
            real_chatrooms,
            chatroom_participants,
            real_participants,
            participants: participants_map,
            tapbacks,
            options,
            offset: get_offset(),
            data_source,
        })
    }

    pub fn conversation(&self, message: &Message) -> Option<(&Chat, &i32)> {
        match message.chat_id.or(message.deleted_from) {
            Some(chat_id) => {
                if let Some(chatroom) = self.chatrooms.get(&chat_id) {
                    self.real_chatrooms.get(&chat_id).map(|id| (chatroom, id))
                } else {
                    eprintln!("Chat ID {chat_id} does not exist in chat table!");
                    None
                }
            }
            None => None,
        }
    }

    pub fn who<'a, 'b: 'a>(
        &'a self,
        handle_id: Option<i32>,
        is_from_me: bool,
        destination_caller_id: &'b Option<String>,
    ) -> &'a str {
        if is_from_me {
            if self.options.use_caller_id {
                return destination_caller_id.as_deref().unwrap_or(ME);
            }
            return ME;
        } else if let Some(handle_id) = handle_id {
            return match self.resolve_participant(handle_id) {
                Some(contact) => contact.get_display_name(),
                None => UNKNOWN,
            };
        }
        UNKNOWN
    }

    pub fn resolve_participant(&self, handle_id: i32) -> Option<&Name> {
        self.real_participants
            .get(&handle_id)
            .and_then(|internal_id| self.participants.get(internal_id))
    }

    /// Resolve a comma-separated participant filter into chat and handle IDs.
    pub fn resolve_filtered_handles(&mut self) {
        if let Some(conversation_filter) = &self.options.conversation_filter {
            let parsed_handle_filter = conversation_filter.split(',').collect::<Vec<&str>>();

            let mut included_chatrooms: BTreeSet<i32> = BTreeSet::new();
            let mut included_handles: BTreeSet<i32> = BTreeSet::new();

            self.participants.iter().for_each(|(_, handle_name)| {
                for included_name in &parsed_handle_filter {
                    if handle_name.contains(included_name) {
                        included_handles.extend(&handle_name.handle_ids);
                    }
                }
            });

            self.chatroom_participants
                .iter()
                .for_each(|(chat_id, participants)| {
                    if !participants.is_disjoint(&included_handles) {
                        included_chatrooms.insert(*chat_id);
                    }
                });

            self.options
                .query_context
                .set_selected_handle_ids(included_handles);

            self.options
                .query_context
                .set_selected_chat_ids(included_chatrooms);

            self.log_filtered_handles_and_chats();
        }
    }

    fn log_filtered_handles_and_chats(&self) {
        if let (Some(selected_handle_ids), Some(selected_chat_ids)) = (
            &self.options.query_context.selected_handle_ids,
            &self.options.query_context.selected_chat_ids,
        ) {
            let unique_handle_ids: HashSet<Option<&i32>> = selected_handle_ids
                .iter()
                .map(|handle_id| self.real_participants.get(handle_id))
                .collect();
            eprintln!(
                "Selected {} handle{} from {} chat{} from filter `{}`",
                unique_handle_ids.len(),
                plural(unique_handle_ids.len()),
                selected_chat_ids.len(),
                plural(selected_chat_ids.len()),
                self.options
                    .conversation_filter
                    .as_deref()
                    .unwrap_or_default()
            );
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

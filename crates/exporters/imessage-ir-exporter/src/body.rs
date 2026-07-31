//! Body parse + GUID-based attachment index resolution.

use std::collections::{HashMap, HashSet};

use imessage_database::tables::{
    attachment::Attachment,
    messages::{
        Message,
        models::{AttributedRange, BubbleComponent},
    },
};
use rusqlite::Connection;

/// Apply typedstream body when parse succeeds (fills `components` / text).
pub(crate) fn apply_body(msg: &mut Message, db: &Connection) {
    if let Ok(body) = msg.parse_body(db) {
        msg.apply_body(body);
    }
}

pub(crate) struct AttachmentResolver {
    by_guid: HashMap<String, usize>,
    next_positional: usize,
}

impl AttachmentResolver {
    pub(crate) fn new(attachments: &[Attachment]) -> Self {
        Self {
            by_guid: attachments
                .iter()
                .enumerate()
                .filter_map(|(i, a)| a.guid.clone().map(|g| (g, i)))
                .collect(),
            next_positional: 0,
        }
    }

    pub(crate) fn resolve(&mut self, range: &AttributedRange) -> usize {
        if let Some(idx) = range
            .attachment
            .as_ref()
            .and_then(|meta| meta.guid.as_deref())
            .and_then(|guid| self.by_guid.get(guid).copied())
        {
            return idx;
        }
        let idx = self.next_positional;
        self.next_positional += 1;
        idx
    }
}

pub(crate) fn resolve_run<'r>(
    ranges: &'r [AttributedRange],
    resolver: &mut AttachmentResolver,
) -> Vec<(&'r AttributedRange, Option<usize>)> {
    ranges
        .iter()
        .map(|range| {
            let idx = range.attachment.is_some().then(|| resolver.resolve(range));
            (range, idx)
        })
        .collect()
}

/// Indices into `attachments` referenced by the message body.
///
/// When `components` is empty (parse failure), falls back to every join row.
pub(crate) fn referenced_attachment_indices(message: &Message, attachments: &[Attachment]) -> Vec<usize> {
    if attachments.is_empty() {
        return Vec::new();
    }

    if message.components.is_empty() {
        return (0..attachments.len()).collect();
    }

    let mut resolver = AttachmentResolver::new(attachments);
    let mut indices = HashSet::new();

    for (part_idx, component) in message.components.iter().enumerate() {
        match component {
            BubbleComponent::Run(ranges) => {
                if message.is_part_edited(part_idx) {
                    continue;
                }
                for (_, idx) in resolve_run(ranges, &mut resolver) {
                    if let Some(i) = idx
                        && i < attachments.len()
                    {
                        indices.insert(i);
                    }
                }
            }
            BubbleComponent::App | BubbleComponent::Retracted => {}
        }
    }

    let mut out: Vec<_> = indices.into_iter().collect();
    out.sort_unstable();
    out
}

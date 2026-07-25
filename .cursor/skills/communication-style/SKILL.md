# Communication Style

This skill controls **how every explanation, design document, code review, README, issue, and recommendation is written.**

The goal is simple:

> Write for an experienced software engineer who has **never seen this project before**.

The reader should understand every recommendation without already knowing the codebase.

---

# The Golden Rule

Assume the reader has zero project context.

Every recommendation must answer three questions:

1. What should change?
2. Why should it change?
3. What problem does it solve?

Never assume the reader already knows the answer.

---

# Explain Before Naming

Do not introduce project-specific terms without immediately explaining them.

Bad:

> Normalize the owner model.

Good:

> The converters currently represent the owner in different ways. Make every converter use the same representation so they behave consistently.

---

# Never Write in Review-Note Shorthand

Do not write like an issue title, TODO list, commit message, or code review heading.

Avoid output like:

- Atomic staging writes
- CLI parity
- Hardening
- Docs cleanup
- XML streaming
- Attachment sanitization

Instead write complete thoughts.

Example:

Instead of

> Atomic staging writes

write

> Write every CSV to a temporary file first. Rename the temporary file after the write succeeds. This prevents crashes from leaving behind empty or partially-written files.

---

# Every Recommendation Must Be Complete

Each recommendation must contain:

- what changes
- why it changes
- what benefit it provides

Never stop after naming an idea.

Bad:

> CLI parity

Good:

> Make `--owner-phone` repeatable in `sms-backup-plus-to-csv`. The library already accepts multiple phone numbers, but the command-line interface only forwards one. This makes all converters behave the same way.

---

# Prefer Verbs Over Nouns

Describe actions.

Avoid noun-heavy engineering phrases.

Instead of

> phone normalization

write

> Make every converter use the same phone-number rules.

Instead of

> attachment sanitization

write

> Remove directory names from attachment filenames before saving them.

Instead of

> dedupe logic

write

> Detect duplicate files by comparing their content hashes.

---

# Never Compress Ideas

Do not replace an explanation with a short engineering phrase.

Forbidden:

- parity
- hardening
- hygiene
- cleanup
- convergence
- normalization
- canonicalization
- emit
- bootstrap
- instantiate
- leverage
- utilize
- synthesize

Always describe the actual work.

---

# Do Not Optimize For Brevity

Optimization is forbidden.

Do not shorten explanations into labels.

Longer, clearer explanations are always preferred over clever wording.

If a sentence can be expanded to make the intent clearer, expand it.

---

# One Idea Per Sentence

Avoid dense sentences.

Bad:

> Normalize owner handling while fixing CLI parity and staging writes.

Good:

> Make every converter use the same owner representation.

> Make every command-line interface accept the same options.

> Write output to temporary files before renaming them.

---

# Use Concrete Examples

Whenever practical, use realistic examples.

Instead of

> sanitize filenames

write

> A backup could contain an attachment named `../../notes.txt`. Strip directory names before saving the attachment so every file stays inside the output directory.

Instead of

> duplicate filenames

write

> Two photos named `IMG_0001.jpg` should not overwrite each other. Name attachments using a hash of their contents.

---

# Explain Design Decisions Honestly

When something is a design choice, explain why.

Do not write

> This is out of scope.

Instead write

> This was not implemented because the tools only process personal backups. Supporting hostile input would add significant complexity for little benefit.

Do not write

> We chose...

Instead write

> This approach keeps the implementation simpler because...

---

# Write As a Tool

Never use:

- we
- us
- our

Use one of these instead:

- the tool
- the converter
- the parser
- matching uses...
- parsing does...
- output is written...

Or simply write the sentence without a subject.

---

# Define Technical Terms

The first time a technical term appears, explain it in plain English.

Example:

> A hash is a short fingerprint calculated from a file's contents.

After that, "hash" may be used normally.

---

# Prefer Plain English

Replace jargon whenever possible.

| Avoid | Use Instead |
|--------|-------------|
| canonical | the one standard version |
| emit | write |
| synthesize | create |
| instantiate | create |
| bootstrap | set up |
| leverage | use |
| utilize | use |
| hygiene | cleanup |
| parity | behave the same way |
| normalization | use the same format |
| canonicalization | convert to one standard format |

If a simpler phrase exists, use it.

---

# Structure

For explanations longer than a few paragraphs:

- Start with a one-sentence summary.
- Use headings.
- Use numbered lists when describing steps.
- Use bullet lists for independent items.
- Avoid walls of text.

---

# Code Reviews

Treat every recommendation like a GitHub review comment.

Do not merely identify a problem.

Always explain:

- what should change
- why
- the expected result

---

# Documentation

Documentation should teach.

Do not assume the reader already understands the implementation.

Introduce concepts before discussing implementation details.

Use examples liberally.

---

# Before Responding

Read the response one final time.

Check for:

- issue-title style headings
- unexplained jargon
- compressed engineering shorthand
- noun-heavy phrases
- unnecessary abbreviations
- uses of "we", "us", or "our"

Rewrite anything that requires project knowledge to understand.

The reader should never need to mentally expand a sentence to understand what it means.

If there is a choice between writing something shorter or making it easier to understand, always choose the clearer explanation.

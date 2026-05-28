// Minimal inline-markdown renderer for chat messages.
//
// Handles only the four things people actually use in chat:
//   **bold**   __bold__
//   *italic*   _italic_
//   `code`
//   https://… autolinks (recognized in plain text, not inside code spans)
//
// Returns an array of React-ready nodes (strings or React elements).
// Caller renders them inline; we don't parse block-level constructs
// like lists / headings — those would change layout, which is not
// what people want in a chat row.
//
// Built ad-hoc with regex passes. Not a real CommonMark parser; that
// would pull a 30 KB dep. The grammar we accept is what 99% of chat
// messages need, and the renderer fails safe on anything weird (just
// leaves the source text as plain).

import { createElement, Fragment } from "react";

// URL pattern — conservative. Anchored at word boundary so "foo.bar"
// inside identifiers stays plain. Trailing punctuation (.,;:?!) is
// excluded from the match so "see https://example.com." doesn't
// swallow the period.
const URL_RE = /\bhttps?:\/\/[^\s<>"]+[^\s<>".,;:?!)]/g;

// Split a string on a paired delimiter, returning [outside, inside,
// outside, inside, …]. Empty inside segments mean a stray unpaired
// delimiter — the parser leaves them as literal characters.
const splitPaired = (text, openClose) => {
  const parts = [];
  let i = 0;
  while (i < text.length) {
    const open = text.indexOf(openClose, i);
    if (open < 0) { parts.push(text.slice(i), null); break; }
    const close = text.indexOf(openClose, open + openClose.length);
    if (close < 0) { parts.push(text.slice(i), null); break; }
    parts.push(text.slice(i, open), text.slice(open + openClose.length, close));
    i = close + openClose.length;
  }
  return parts;
};

// Render a plain-text run (no markers) into nodes, autolinking URLs.
const renderPlain = (text, keyBase) => {
  const out = [];
  let last = 0;
  let i = 0;
  text.replace(URL_RE, (match, idx) => {
    if (idx > last) out.push(text.slice(last, idx));
    out.push(
      createElement(
        "a",
        {
          key: `${keyBase}-url-${i++}`,
          href: match,
          target: "_blank",
          rel: "noopener noreferrer",
          className: "underline decoration-amber-500/60 underline-offset-2 hover:text-amber-600 dark:hover:text-amber-400",
        },
        match,
      ),
    );
    last = idx + match.length;
    return match;
  });
  if (last < text.length) out.push(text.slice(last));
  return out;
};

// Pull out `code` spans first — they MUST come before bold/italic so
// the markers inside code are preserved literally. After that, run a
// second pass for bold (** or __) then italic (* or _).
const splitCode = (text) => {
  // Backtick-delimited code spans. Singular backticks; not full
  // CommonMark's "any number of backticks" rule — chat doesn't need it.
  return splitPaired(text, "`");
};

const renderInlineNoCode = (text, keyBase) => {
  // Bold first: ** or __
  const boldParts = [];
  {
    const boldOnly = (t, m) => {
      const parts = splitPaired(t, m);
      for (let i = 0; i < parts.length; i++) {
        if (i % 2 === 0) boldParts.push({ kind: "plain", text: parts[i] });
        else if (parts[i] != null) boldParts.push({ kind: "bold", text: parts[i] });
      }
    };
    // Run on the whole input twice — first ** then __ — to keep
    // things simple. ** dominates if both appear.
    const tmp = [];
    splitPaired(text, "**").forEach((seg, i) => {
      if (i % 2 === 0) tmp.push({ kind: "plain", text: seg });
      else if (seg != null) tmp.push({ kind: "bold", text: seg });
    });
    // Now apply __ on each remaining plain segment.
    for (const piece of tmp) {
      if (piece.kind === "bold") boldParts.push(piece);
      else {
        const sub = splitPaired(piece.text, "__");
        sub.forEach((seg, i) => {
          if (i % 2 === 0) boldParts.push({ kind: "plain", text: seg });
          else if (seg != null) boldParts.push({ kind: "bold", text: seg });
        });
      }
    }
  }

  // Italic pass on each plain segment from the bold pass.
  const italicParts = [];
  for (const piece of boldParts) {
    if (piece.kind === "bold") italicParts.push(piece);
    else {
      const sub1 = splitPaired(piece.text, "*");
      const stage1 = [];
      sub1.forEach((seg, i) => {
        if (i % 2 === 0) stage1.push({ kind: "plain", text: seg });
        else if (seg != null) stage1.push({ kind: "italic", text: seg });
      });
      for (const inner of stage1) {
        if (inner.kind === "italic") italicParts.push(inner);
        else {
          const sub2 = splitPaired(inner.text, "_");
          sub2.forEach((seg, i) => {
            if (i % 2 === 0) italicParts.push({ kind: "plain", text: seg });
            else if (seg != null) italicParts.push({ kind: "italic", text: seg });
          });
        }
      }
    }
  }

  // Final nodes — autolink the plain segments.
  const nodes = [];
  italicParts.forEach((piece, idx) => {
    const k = `${keyBase}-${idx}`;
    if (piece.kind === "bold") {
      nodes.push(createElement("strong", { key: k, className: "font-semibold" }, ...renderPlain(piece.text, k)));
    } else if (piece.kind === "italic") {
      nodes.push(createElement("em", { key: k, className: "italic" }, ...renderPlain(piece.text, k)));
    } else if (piece.text) {
      nodes.push(...renderPlain(piece.text, k));
    }
  });
  return nodes;
};

export const renderMarkdown = (text, keyBase = "md") => {
  if (typeof text !== "string" || !text) return [text || ""];
  const codeParts = splitCode(text);
  const nodes = [];
  codeParts.forEach((seg, i) => {
    const k = `${keyBase}-c${i}`;
    if (i % 2 === 1 && seg != null) {
      // code span — render verbatim, no further parsing.
      nodes.push(
        createElement(
          "code",
          {
            key: k,
            className: "rounded bg-zinc-200/70 dark:bg-zinc-700/60 px-1.5 py-0.5 text-[12.5px] font-mono",
          },
          seg,
        ),
      );
    } else if (seg) {
      nodes.push(...renderInlineNoCode(seg, k));
    }
  });
  return nodes.length ? nodes : [text];
};

// Convenience: returns a Fragment ready to drop into JSX.
export const Markdown = ({ children, keyBase = "md" }) => {
  return createElement(Fragment, null, ...renderMarkdown(children, keyBase));
};

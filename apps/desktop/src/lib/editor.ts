// The writing surface: CodeMirror 6 set up for prose rather than code. Markdown stays plain text
// on disk; here headings, emphasis, links and notes-to-self are styled so the page reads cleanly.

import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { search, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, type Range, type Transaction } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  keymap,
  placeholder,
  ViewPlugin,
  type ViewUpdate,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";

const prose = HighlightStyle.define([
  { tag: tags.heading1, fontSize: "1.55em", fontWeight: "600", lineHeight: "1.3" },
  { tag: tags.heading2, fontSize: "1.3em", fontWeight: "600" },
  { tag: [tags.heading3, tags.heading4, tags.heading5, tags.heading6], fontWeight: "600" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strong, fontWeight: "700" },
  { tag: tags.strikethrough, textDecoration: "line-through" },
  // Markdown's own marks (#, *, >) recede so the words stand out.
  { tag: tags.processingInstruction, color: "var(--muted)" },
  { tag: tags.quote, fontStyle: "italic", color: "var(--text-soft)" },
  { tag: [tags.link, tags.url], color: "var(--accent)" },
  { tag: tags.monospace, fontFamily: "var(--mono)", fontSize: "0.85em" },
]);

const comment = Decoration.mark({ class: "st-comment" });
const wikilink = Decoration.mark({ class: "st-link" });

/** Marks `%% notes to self %%` (which word counts skip) and `[[links]]`. */
function decorate(view: EditorView): DecorationSet {
  const text = view.state.doc.toString();
  const ranges: Range<Decoration>[] = [];
  for (let from = text.indexOf("%%"); from >= 0; from = text.indexOf("%%", from)) {
    const close = text.indexOf("%%", from + 2);
    const to = close < 0 ? text.length : close + 2;
    ranges.push(comment.range(from, to));
    from = to;
  }
  for (const match of text.matchAll(/\[\[[^\]\n]+\]\]/g)) {
    ranges.push(wikilink.range(match.index, match.index + match[0].length));
  }
  return Decoration.set(ranges, true);
}

const marks = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = decorate(view);
    }
    update(update: ViewUpdate) {
      if (update.docChanged) this.decorations = decorate(update.view);
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

/**
 * Typewriter scrolling: whatever moves the caret from the keyboard (typing, deleting, undoing, moving the cursor)
 * also scrolls its line to the middle of the window, so the eyes stay in one place. Clicks and drags with the mouse
 * don't, so the text never jumps under the pointer. The scroll rides along with the change itself.
 */
const fromKeyboard = (tr: Transaction) =>
  ["input", "delete", "undo", "redo"].some((event) => tr.isUserEvent(event)) ||
  (tr.isUserEvent("select") && !tr.isUserEvent("select.pointer"));

const typewriter = [
  EditorState.transactionExtender.of((tr) =>
    (tr.docChanged || tr.selection) && fromKeyboard(tr)
      ? { effects: EditorView.scrollIntoView(tr.newSelection.main.head, { y: "center" }) }
      : null,
  ),
  // Room above the first line too, so it starts at the same height.
  EditorView.contentAttributes.of({ class: "st-typewriter" }),
];

const theme = EditorView.theme({
  "&": { height: "100%", fontSize: "var(--editor-size)", color: "var(--text)", backgroundColor: "transparent" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "var(--serif)", lineHeight: "1.75", overflow: "auto" },
  // A comfortable measure, and room to keep writing near the middle of the screen.
  ".cm-content": {
    maxWidth: "36em",
    margin: "0 auto",
    padding: "3.5rem 2rem 45vh",
    caretColor: "var(--accent)",
  },
  ".cm-content.st-typewriter": { paddingTop: "45vh" },
  ".cm-line": { padding: "0" },
  ".cm-placeholder": { color: "var(--muted)", fontStyle: "italic" },
  ".st-comment": {
    color: "var(--muted)",
    fontFamily: "var(--sans)",
    fontSize: "0.82em",
    fontStyle: "normal",
    fontWeight: "normal",
  },
  ".st-link": { color: "var(--accent)" },
  ".cm-panels": { backgroundColor: "var(--panel)", color: "var(--text)", fontFamily: "var(--sans)" },
  ".cm-panels-top": { borderBottom: "1px solid var(--line)" },
  ".cm-searchMatch": { backgroundColor: "var(--selection)" },
  ".cm-searchMatch-selected": { outline: "1px solid var(--accent)" },
});

export interface ProseEditor {
  /** Replaces the text, starting a fresh undo history. */
  load(text: string): void;
  /** Turns typewriter scrolling on or off. */
  setTypewriter(on: boolean): void;
  focus(): void;
  destroy(): void;
}

export function createEditor(parent: HTMLElement, onChange: (text: string) => void): ProseEditor {
  const typewriting = new Compartment();
  let typewriterOn = false;
  const extensions = () => [
    typewriting.of(typewriterOn ? typewriter : []),
    history(),
    keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]),
    search({ top: true }),
    markdown({ base: markdownLanguage }),
    syntaxHighlighting(prose),
    marks,
    EditorView.lineWrapping,
    EditorView.contentAttributes.of({ spellcheck: "true", autocorrect: "on", autocapitalize: "sentences" }),
    placeholder("Begin here…"),
    theme,
    EditorView.updateListener.of((update) => {
      if (update.docChanged) onChange(update.state.doc.toString());
    }),
  ];
  const view = new EditorView({ parent, state: EditorState.create({ doc: "", extensions: extensions() }) });
  const centre = () => view.dispatch({ effects: EditorView.scrollIntoView(view.state.selection.main.head, { y: "center" }) });
  return {
    load(text) {
      view.setState(EditorState.create({ doc: text, extensions: extensions() }));
    },
    setTypewriter(on) {
      if (on === typewriterOn) return;
      typewriterOn = on;
      view.dispatch({ effects: typewriting.reconfigure(on ? typewriter : []) });
      if (on) requestAnimationFrame(centre);
    },
    focus() {
      view.focus();
    },
    destroy() {
      view.destroy();
    },
  };
}

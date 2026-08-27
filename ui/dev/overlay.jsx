// The Agentation toolbar, mounted onto a page that knows nothing about React.
//
// `ui/index.html` is deliberately one self-contained document with no build
// step — see the comment at the top of it, and `tests/ui.rs`, which fails if a
// external reference ever appears in it. So the toolbar is *overlaid* at serve
// time by the dev server next door rather than added to the page, and the file
// that ships stays the file that was reviewed.
import React from "react";
import { createRoot } from "react-dom/client";
import { Agentation } from "agentation";

const host = document.createElement("div");
host.id = "agentation-root";
document.body.appendChild(host);

// `endpoint` is Agent Sync: annotations post to the local agentation-mcp
// server, which the coding agent reads. Without it the toolbar still works —
// "Copy" puts structured markdown on the clipboard to paste into a chat.
createRoot(host).render(
  React.createElement(Agentation, {
    endpoint: "http://localhost:4747",
    onSessionCreated: (id) => console.log("[agentation] session", id),
  }),
);

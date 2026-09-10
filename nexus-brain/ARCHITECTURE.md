# NexusBrain — Architecture & Vision

NexusBrain is the **Knowledge IDE** designed to complement the Code Explorer ecosystem. While Code Explorer-RS focuses on the high-performance extraction of knowledge from source code, NexusBrain provides the interactive environment to curate, edit, and navigate that knowledge.

## 🏗️ The Split Strategy

To ensure stability and speed of delivery, the ecosystem is split into two specialized repositories:

1.  **Code Explorer-RS (The Engine)**:
    *   **Role**: Ingestion, Graph Construction, LLM Enrichment, Export.
    *   **Status**: Production-ready.
    *   **Output**: Standardized "Digital Brain" Markdown Vaults (Obsidian compatible).

2.  **NexusBrain (The Workbench)**:
    *   **Role**: Specialized Editor, Graph Visualization, Knowledge Lifecycle Management.
    *   **Status**: In development.
    *   **Input**: Consumes Markdown Vaults produced by Code Explorer.

## 🌟 Key Features of NexusBrain

*   **Bidirectional Knowledge**: Seamlessly navigate from a high-level business process description down to the technical code symbol note.
*   **Knowledge Linting**: Integrated tools to find "thin" documentation, broken links, or undocumented code areas identified by the Code Explorer engine.
*   **AI-Native Editor**: Deep integration with Claude/Gemini to help human curators "flesh out" the knowledge base directly within the app.
*   **High-Performance Graph**: Interactive WebGL-based visualization of the knowledge base, allowing for rapid mental mapping of complex systems.

## 🛠️ Tech Stack

*   **Backend**: Rust + Tauri v2 (Low level file access, performance).
*   **Frontend**: React 18 + Tailwind CSS (Modern, fluid UI).
*   **Data**: Native Markdown files (No vendor lock-in).
*   **Editor**: Md-Editor-RT (Full-featured Markdown editing with preview).

---

*This project is maintained by Agile Up as part of the Software Intelligence suite.*

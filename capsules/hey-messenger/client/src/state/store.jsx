// Minimal client-side state — just enough to drive the UI shell during
// Phase 3. Real state lives in iroh-docs (Phase 4) + Carrier (Phase 3+).
//
// Uses a single React context with a reducer; no extra deps.

import { createContext, useContext, useReducer, useCallback } from "react";
import {
  workspaces,
  channelsByWorkspace,
  dmsByWorkspace,
  messagesByThread,
  currentUser,
} from "../data/mock.js";

const StoreCtx = createContext(null);

const init = {
  workspaces,
  channelsByWorkspace,
  dmsByWorkspace,
  messages: { ...messagesByThread },
  activeWorkspaceId: workspaces[0].id,
  activeThreadId: channelsByWorkspace[workspaces[0].id][0].id,
  inspectorOpen: true,
  currentUser,
  // Per-thread search query. Cleared automatically when the user
  // switches threads so the filter doesn't carry over confusingly.
  searchQuery: "",
};

const reducer = (state, action) => {
  switch (action.type) {
    case "set-workspace": {
      const wsId = action.id;
      const firstChannel = (state.channelsByWorkspace[wsId] || [])[0];
      const firstDm = (state.dmsByWorkspace[wsId] || [])[0];
      const next = firstChannel?.id || firstDm?.id || null;
      return { ...state, activeWorkspaceId: wsId, activeThreadId: next };
    }
    case "set-thread":
      // Reset search when changing threads — feels jarring otherwise.
      return { ...state, activeThreadId: action.id, searchQuery: "" };
    case "toggle-inspector":
      return { ...state, inspectorOpen: !state.inspectorOpen };
    case "set-search":
      return { ...state, searchQuery: action.query || "" };
    case "append-message": {
      const { threadId, message } = action;
      const prior = state.messages[threadId] || [];
      return {
        ...state,
        messages: { ...state.messages, [threadId]: [...prior, message] },
      };
    }
    default:
      return state;
  }
};

export const StoreProvider = ({ children }) => {
  const [state, dispatch] = useReducer(reducer, init);
  const setWorkspace = useCallback((id) => dispatch({ type: "set-workspace", id }), []);
  const setThread    = useCallback((id) => dispatch({ type: "set-thread", id }),    []);
  const toggleInspector = useCallback(() => dispatch({ type: "toggle-inspector" }), []);
  const setSearch = useCallback((query) => dispatch({ type: "set-search", query }), []);
  const appendMessage = useCallback(
    (threadId, message) => dispatch({ type: "append-message", threadId, message }),
    [],
  );
  return (
    <StoreCtx.Provider value={{ state, setWorkspace, setThread, toggleInspector, setSearch, appendMessage }}>
      {children}
    </StoreCtx.Provider>
  );
};

export const useStore = () => {
  const ctx = useContext(StoreCtx);
  if (!ctx) throw new Error("useStore must be inside <StoreProvider>");
  return ctx;
};

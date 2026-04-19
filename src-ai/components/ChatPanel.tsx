import { useState, useCallback, useRef, useEffect } from "react";
import { aiApi } from "../lib/api-ai";
import type { ChatResponse, ChatSession, QueryResult } from "../lib/types-ai";
import { SimpleMarkdown } from "./SimpleMarkdown";

function errMsg(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  try { return JSON.stringify(e); } catch { return String(e); }
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  sql?: string | null;
  queryResult?: QueryResult | null;
  timestamp: number;
}

export interface ChatPanelProps {
  fileId: string | null;
  onProcessing: (loading: boolean) => void;
}

function MiniTable({ result }: { result: QueryResult }) {
  const maxRows = 8;
  const displayRows = result.rows.slice(0, maxRows);
  return (
    <div className="chat-mini-table-wrap">
      <table className="chat-mini-table">
        <thead>
          <tr>
            {result.columns.map((col) => (
              <th key={col}>{col}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {displayRows.map((row, ri) => (
            <tr key={ri}>
              {row.map((cell, ci) => (
                <td key={ci}>{cell == null ? "" : String(cell)}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {result.row_count > maxRows && (
        <div className="chat-mini-table-more">
          …and {result.row_count - maxRows} more rows
        </div>
      )}
    </div>
  );
}

function MessageBubble({ msg }: { msg: ChatMessage }) {
  return (
    <div className={`chat-bubble ${msg.role}`}>
      {msg.role === "assistant" ? (
        <SimpleMarkdown content={msg.content} />
      ) : (
        <span>{msg.content}</span>
      )}
      {msg.sql && (
        <div className="chat-sql-block">
          <div className="chat-sql-label">SQL</div>
          <pre><code>{msg.sql}</code></pre>
        </div>
      )}
      {msg.queryResult && msg.queryResult.row_count > 0 && (
        <MiniTable result={msg.queryResult} />
      )}
    </div>
  );
}

export function ChatPanel({ fileId, onProcessing }: ChatPanelProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [session, setSession] = useState<ChatSession | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Auto-scroll on new messages
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const startNewSession = useCallback(async () => {
    if (!fileId) return;
    setError(null);
    setLoading(true);
    onProcessing(true);
    try {
      const s = await aiApi.newChatSession(fileId);
      setSession(s);
      setMessages([]);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setLoading(false);
      onProcessing(false);
    }
  }, [fileId, onProcessing]);

  const sendMessage = useCallback(async () => {
    const text = input.trim();
    if (!text || !fileId) return;

    setInput("");
    setError(null);

    // Ensure a session exists
    let currentSession = session;
    if (!currentSession) {
      setLoading(true);
      onProcessing(true);
      try {
        currentSession = await aiApi.newChatSession(fileId);
        setSession(currentSession);
      } catch (e) {
        setError(errMsg(e));
        setLoading(false);
        onProcessing(false);
        return;
      }
    }

    const userMsg: ChatMessage = {
      id: `u-${Date.now()}`,
      role: "user",
      content: text,
      timestamp: Date.now(),
    };
    setMessages((prev) => [...prev, userMsg]);
    setLoading(true);
    onProcessing(true);

    try {
      const response: ChatResponse = await aiApi.chatMessage(
        fileId,
        currentSession.id,
        text,
      );
      const assistantMsg: ChatMessage = {
        id: `a-${Date.now()}`,
        role: "assistant",
        content: response.content,
        sql: response.sql_executed,
        queryResult: response.query_result,
        timestamp: Date.now(),
      };
      setMessages((prev) => [...prev, assistantMsg]);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setLoading(false);
      onProcessing(false);
    }
  }, [input, fileId, session, onProcessing]);

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        void sendMessage();
      }
    },
    [sendMessage],
  );

  return (
    <div className="ai-panel chat-panel">
      <div className="ai-panel-header chat-header">
        <h3>Chat</h3>
        <button
          className="new-chat-btn"
          onClick={() => void startNewSession()}
          disabled={loading || !fileId}
          title="Start a new conversation"
        >
          New conversation
        </button>
      </div>

      {!fileId ? (
        <div className="ai-empty-state">Open a CSV file to start chatting.</div>
      ) : (
        <>
          <div className="chat-messages">
            {messages.length === 0 && !loading && (
              <div className="chat-empty">
                Ask anything about your data. Try:
                <ul>
                  <li>"What are the top 5 values in column X?"</li>
                  <li>"Show me rows where revenue exceeds 10000."</li>
                  <li>"Summarize the distribution of dates."</li>
                </ul>
              </div>
            )}
            {messages.map((msg) => (
              <MessageBubble key={msg.id} msg={msg} />
            ))}
            {loading && (
              <div className="chat-bubble assistant chat-thinking">
                <span className="ai-dots"><span /><span /><span /></span>
              </div>
            )}
            <div ref={bottomRef} />
          </div>

          {error && (
            <div className="ai-error-banner">
              {error}
              <button
                className="error-dismiss"
                onClick={() => setError(null)}
                aria-label="Dismiss"
              >
                ×
              </button>
            </div>
          )}

          <div className="chat-input-row">
            <textarea
              ref={inputRef}
              className="chat-textarea"
              placeholder={
                loading ? "Waiting for response…" : "Ask a question… (Enter to send, Shift+Enter for newline)"
              }
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={onKeyDown}
              disabled={loading}
              rows={2}
              aria-label="Chat message"
            />
            <button
              className="chat-send-btn primary"
              onClick={() => void sendMessage()}
              disabled={loading || !input.trim()}
              aria-label="Send message"
            >
              Send
            </button>
          </div>
        </>
      )}
    </div>
  );
}

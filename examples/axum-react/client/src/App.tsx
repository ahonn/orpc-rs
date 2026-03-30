import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { orpc, client } from "./rpc";
import type { Planet, UploadResult } from "./rpc";

// ---------------------------------------------------------------------------
// RPC Protocol demos (via @orpc/client RPCLink)
// ---------------------------------------------------------------------------

function PingButton() {
  const [result, setResult] = useState<string | null>(null);

  async function handlePing() {
    const res: string = await client.ping();
    setResult(res);
  }

  return (
    <div style={{ marginBottom: 24 }}>
      <h3>Ping (RPC)</h3>
      <button onClick={handlePing}>Ping Server</button>
      {result && <pre>Response: {result}</pre>}
    </div>
  );
}

function PlanetList() {
  const { data, isLoading, error } = useQuery<Planet[]>({
    ...orpc.planet.list.queryOptions({}),
  });

  if (isLoading) return <p>Loading planets...</p>;
  if (error) return <p style={{ color: "red" }}>Error: {error.message}</p>;

  return (
    <div style={{ marginBottom: 24 }}>
      <h3>Planet List (RPC + TanStack Query)</h3>
      <table border={1} cellPadding={8} style={{ borderCollapse: "collapse" }}>
        <thead>
          <tr>
            <th>ID</th>
            <th>Name</th>
            <th>Radius (km)</th>
            <th>Rings</th>
          </tr>
        </thead>
        <tbody>
          {data?.map((p: Planet) => (
            <tr key={p.id}>
              <td>{p.id}</td>
              <td>{p.name}</td>
              <td>{p.radius_km.toLocaleString()}</td>
              <td>{p.has_rings ? "Yes" : "No"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function FindPlanet() {
  const [name, setName] = useState("");
  const [result, setResult] = useState<Planet | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleSearch(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setResult(null);
    try {
      const planet: Planet = await client.planet.find({ name });
      setResult(planet);
    } catch (err: any) {
      setError(err?.message ?? "Unknown error");
    }
  }

  return (
    <div style={{ marginBottom: 24 }}>
      <h3>Find Planet (RPC)</h3>
      <form onSubmit={handleSearch}>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Planet name (e.g. Earth)"
        />
        <button type="submit" style={{ marginLeft: 8 }}>Search</button>
      </form>
      {result && <pre>{JSON.stringify(result, null, 2)}</pre>}
      {error && <p style={{ color: "red" }}>{error}</p>}
    </div>
  );
}

function CreatePlanet() {
  const queryClient = useQueryClient();
  const [result, setResult] = useState<Planet | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function handleCreate(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setResult(null);
    setLoading(true);
    try {
      const form = e.target as HTMLFormElement;
      const data = new FormData(form);
      const planet: Planet = await client.planet.create({
        name: data.get("name") as string,
        radius_km: Number(data.get("radius_km")),
        has_rings: data.get("has_rings") === "on",
      });
      setResult(planet);
      queryClient.invalidateQueries({ queryKey: orpc.planet.list.key() });
    } catch (err: any) {
      setError(err?.message ?? "Unknown error");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div style={{ marginBottom: 24 }}>
      <h3>Create Planet (RPC Mutation)</h3>
      <form onSubmit={handleCreate}>
        <div>
          <input name="name" placeholder="Name" required />
        </div>
        <div style={{ marginTop: 4 }}>
          <input name="radius_km" type="number" placeholder="Radius (km)" required />
        </div>
        <div style={{ marginTop: 4 }}>
          <label>
            <input name="has_rings" type="checkbox" /> Has rings
          </label>
        </div>
        <button type="submit" style={{ marginTop: 8 }} disabled={loading}>
          {loading ? "Creating..." : "Create"}
        </button>
      </form>
      {result && <pre>Created: {JSON.stringify(result, null, 2)}</pre>}
      {error && <p style={{ color: "red" }}>{error}</p>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// File Upload demo (multipart/form-data via @orpc/client)
// ---------------------------------------------------------------------------

function FileUpload() {
  const [result, setResult] = useState<UploadResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function handleUpload(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setResult(null);
    setLoading(true);
    try {
      const form = e.target as HTMLFormElement;
      const description = (
        form.querySelector('input[name="description"]') as HTMLInputElement
      ).value;
      const fileInput = form.querySelector(
        'input[type="file"]'
      ) as HTMLInputElement;
      const file = fileInput.files?.[0];
      if (!file) {
        setError("Please select a file");
        return;
      }
      // @orpc/client auto-detects Blob/File fields and switches to multipart
      const res: UploadResult = await client.file.upload({
        description,
        file,
      });
      setResult(res);
    } catch (err: any) {
      setError(err?.message ?? "Upload failed");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div style={{ marginBottom: 24 }}>
      <h3>File Upload (RPC Multipart)</h3>
      <p style={{ color: "#666", fontSize: 14 }}>
        Uses <code>multipart/form-data</code> — <code>@orpc/client</code>{" "}
        detects <code>Blob</code>/<code>File</code> fields and encodes
        automatically.
      </p>
      <form onSubmit={handleUpload}>
        <div>
          <input
            name="description"
            placeholder="File description"
            required
          />
        </div>
        <div style={{ marginTop: 4 }}>
          <input type="file" required />
        </div>
        <button type="submit" style={{ marginTop: 8 }} disabled={loading}>
          {loading ? "Uploading..." : "Upload"}
        </button>
      </form>
      {result && <pre>{JSON.stringify(result, null, 2)}</pre>}
      {error && <p style={{ color: "red" }}>{error}</p>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// SSE Subscription demo (multi-value ProcedureStream → text/event-stream)
// ---------------------------------------------------------------------------

function SseSubscription() {
  const [events, setEvents] = useState<string[]>([]);
  const [status, setStatus] = useState<"idle" | "streaming" | "done" | "error">("idle");

  async function handleSubscribe() {
    setEvents([]);
    setStatus("streaming");

    try {
      const resp = await fetch("/rpc/planet/stream", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: "{}",
      });

      if (resp.headers.get("content-type")?.includes("text/event-stream")) {
        const reader = resp.body!.getReader();
        const decoder = new TextDecoder();
        let buffer = "";

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;

          buffer += decoder.decode(value, { stream: true });
          const lines = buffer.split("\n\n");
          buffer = lines.pop() ?? "";

          for (const block of lines) {
            if (!block.trim()) continue;
            const eventMatch = block.match(/^event: (.+)$/m);
            const dataMatch = block.match(/^data: ?(.*)$/m);
            const eventType = eventMatch?.[1] ?? "unknown";
            const data = dataMatch?.[1] ?? "";

            if (eventType === "message") {
              setEvents((prev) => [...prev, data]);
            } else if (eventType === "done") {
              setStatus("done");
            } else if (eventType === "error") {
              setEvents((prev) => [...prev, `ERROR: ${data}`]);
              setStatus("error");
            }
          }
        }
        if (status === "streaming") setStatus("done");
      } else {
        const text = await resp.text();
        setEvents([`Unexpected response: ${text}`]);
        setStatus("error");
      }
    } catch (err: any) {
      setEvents((prev) => [...prev, `Fetch error: ${err.message}`]);
      setStatus("error");
    }
  }

  return (
    <div style={{ marginBottom: 24 }}>
      <h3>SSE Subscription (RPC Streaming)</h3>
      <p style={{ color: "#666", fontSize: 14 }}>
        Calls <code>POST /rpc/planet/stream</code> — server returns <code>text/event-stream</code>.
        New planets appear here in real-time when created via the form above or the OpenAPI endpoint.
      </p>
      <button onClick={handleSubscribe} disabled={status === "streaming"}>
        {status === "streaming" ? "Listening..." : "Subscribe to Planet Updates"}
      </button>
      {events.length > 0 && (
        <pre style={{ maxHeight: 200, overflow: "auto", background: "#f5f5f5", padding: 8 }}>
          {events.map((e, i) => `[${i}] ${e}`).join("\n")}
        </pre>
      )}
      {status === "done" && <p style={{ color: "green" }}>Stream complete</p>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// OpenAPI Protocol demo (REST-style endpoints)
// ---------------------------------------------------------------------------

function OpenApiDemo() {
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleGetAll() {
    setError(null);
    try {
      const resp = await fetch("/rest/planets");
      const data = await resp.json();
      setResult(JSON.stringify(data, null, 2));
    } catch (err: any) {
      setError(err.message);
    }
  }

  async function handleGetOne(name: string) {
    setError(null);
    try {
      const resp = await fetch(`/rest/planets/${encodeURIComponent(name)}`);
      const data = await resp.json();
      if (!resp.ok) {
        setError(`${data.code}: ${data.message}`);
        setResult(null);
      } else {
        setResult(JSON.stringify(data, null, 2));
      }
    } catch (err: any) {
      setError(err.message);
    }
  }

  async function handleCreate() {
    setError(null);
    try {
      const resp = await fetch("/rest/planets", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: "Pluto", radius_km: 1188, has_rings: false }),
      });
      const data = await resp.json();
      setResult(JSON.stringify(data, null, 2));
    } catch (err: any) {
      setError(err.message);
    }
  }

  return (
    <div style={{ marginBottom: 24 }}>
      <h3>OpenAPI Protocol (REST)</h3>
      <p style={{ color: "#666", fontSize: 14 }}>
        REST-style endpoints routed by HTTP method + path pattern.
        Responses are plain JSON (no <code>{"{"}"json": ...{"}"}</code> envelope).
      </p>
      <div style={{ display: "flex", gap: 8 }}>
        <button onClick={handleGetAll}>GET /rest/planets</button>
        <button onClick={() => handleGetOne("Earth")}>GET /rest/planets/Earth</button>
        <button onClick={() => handleGetOne("Vulcan")}>GET /rest/planets/Vulcan (404)</button>
        <button onClick={handleCreate}>POST /rest/planets (Pluto)</button>
      </div>
      {result && <pre style={{ background: "#f5f5f5", padding: 8 }}>{result}</pre>}
      {error && <p style={{ color: "red" }}>{error}</p>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

export default function App() {
  return (
    <div style={{ maxWidth: 720, margin: "0 auto", padding: 24, fontFamily: "system-ui" }}>
      <h1>orpc-rs + React Example</h1>
      <p style={{ color: "#666" }}>
        React frontend talking to a Rust axum server via three oRPC protocols.
      </p>

      <hr />
      <h2>RPC Protocol</h2>
      <PingButton />
      <PlanetList />
      <FindPlanet />
      <CreatePlanet />

      <hr />
      <h2>File Upload</h2>
      <FileUpload />

      <hr />
      <h2>SSE Subscription</h2>
      <SseSubscription />

      <hr />
      <h2>OpenAPI Protocol</h2>
      <OpenApiDemo />
    </div>
  );
}

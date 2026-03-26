import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { orpc, client } from "./lib/orpc";
import type { Planet } from "./lib/orpc";
import "./App.css";

// ---------------------------------------------------------------------------
// Ping
// ---------------------------------------------------------------------------

function PingButton() {
  const { data, refetch, isFetching } = useQuery(
    orpc.ping.queryOptions({ input: undefined }),
  );

  return (
    <div style={{ marginBottom: 24 }}>
      <h3>Ping (IPC)</h3>
      <button onClick={() => refetch()} disabled={isFetching}>
        {isFetching ? "Pinging..." : "Ping Server"}
      </button>
      {data && <pre>Response: {data}</pre>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Planet List
// ---------------------------------------------------------------------------

function PlanetList() {
  const { data: planets, isLoading } = useQuery(
    orpc.planet.list.queryOptions({ input: undefined }),
  );

  if (isLoading) return <p>Loading planets...</p>;

  return (
    <div style={{ marginBottom: 24 }}>
      <h3>Planet List (IPC)</h3>
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
          {planets?.map((p) => (
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

// ---------------------------------------------------------------------------
// Find Planet
// ---------------------------------------------------------------------------

function FindPlanet() {
  const [name, setName] = useState("");
  const [result, setResult] = useState<Planet | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleSearch(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setResult(null);
    try {
      const planet = await client.planet.find({ name });
      setResult(planet);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Unknown error");
    }
  }

  return (
    <div style={{ marginBottom: 24 }}>
      <h3>Find Planet (IPC)</h3>
      <form onSubmit={handleSearch}>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Planet name (e.g. Earth)"
        />
        <button type="submit" style={{ marginLeft: 8 }}>
          Search
        </button>
      </form>
      {result && <pre>{JSON.stringify(result, null, 2)}</pre>}
      {error && <p style={{ color: "red" }}>{error}</p>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Create Planet
// ---------------------------------------------------------------------------

function CreatePlanet() {
  const queryClient = useQueryClient();
  const mutation = useMutation(
    orpc.planet.create.mutationOptions({
      onSuccess: () => {
        queryClient.invalidateQueries({
          queryKey: orpc.planet.list.queryKey({ input: undefined }),
        });
      },
    }),
  );

  async function handleCreate(e: React.FormEvent) {
    e.preventDefault();
    const form = e.target as HTMLFormElement;
    const data = new FormData(form);
    mutation.mutate({
      name: data.get("name") as string,
      radius_km: Number(data.get("radius_km")),
      has_rings: data.get("has_rings") === "on",
    });
  }

  return (
    <div style={{ marginBottom: 24 }}>
      <h3>Create Planet (IPC Mutation)</h3>
      <form onSubmit={handleCreate}>
        <div>
          <input name="name" placeholder="Name" required />
        </div>
        <div style={{ marginTop: 4 }}>
          <input
            name="radius_km"
            type="number"
            placeholder="Radius (km)"
            required
          />
        </div>
        <div style={{ marginTop: 4 }}>
          <label>
            <input name="has_rings" type="checkbox" /> Has rings
          </label>
        </div>
        <button
          type="submit"
          style={{ marginTop: 8 }}
          disabled={mutation.isPending}
        >
          {mutation.isPending ? "Creating..." : "Create"}
        </button>
      </form>
      {mutation.data && (
        <pre>Created: {JSON.stringify(mutation.data, null, 2)}</pre>
      )}
      {mutation.error && (
        <p style={{ color: "red" }}>{String(mutation.error)}</p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Subscription (via AsyncIterator from TauriLink)
// ---------------------------------------------------------------------------

function PlanetSubscription() {
  const [events, setEvents] = useState<string[]>([]);
  const [status, setStatus] = useState<
    "idle" | "listening" | "done" | "error"
  >("idle");

  async function handleSubscribe() {
    setEvents([]);
    setStatus("listening");

    try {
      const iter = (await client.planet.stream()) as AsyncIterableIterator<Planet>;
      let id = 0;
      for await (const planet of iter) {
        setEvents((prev) => [...prev, `[${id}] ${JSON.stringify(planet)}`]);
        id++;
      }
      setStatus("done");
    } catch (err) {
      setEvents((prev) => [...prev, `Error: ${JSON.stringify(err)}`]);
      setStatus("error");
    }
  }

  return (
    <div style={{ marginBottom: 24 }}>
      <h3>Subscription (IPC Channel)</h3>
      <p style={{ color: "#666", fontSize: 14 }}>
        Streams newly created planets in real-time via Tauri Channel. Click
        subscribe, then create a planet above — it appears here instantly.
      </p>
      <button onClick={handleSubscribe} disabled={status === "listening"}>
        {status === "listening"
          ? "Listening..."
          : "Subscribe to Planet Updates"}
      </button>
      {events.length > 0 && (
        <pre
          style={{
            maxHeight: 200,
            overflow: "auto",
            background: "#f5f5f5",
            padding: 8,
          }}
        >
          {events.join("\n")}
        </pre>
      )}
      {status === "done" && <p style={{ color: "green" }}>Stream complete</p>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

export default function App() {
  return (
    <div
      style={{
        maxWidth: 720,
        margin: "0 auto",
        padding: 24,
        fontFamily: "system-ui",
      }}
    >
      <h1>orpc-rs + Tauri</h1>
      <p style={{ color: "#666" }}>
        Full @orpc/client + @orpc/tanstack-query integration via Tauri IPC.
      </p>

      <hr />
      <h2>RPC via Tauri IPC</h2>
      <PingButton />
      <PlanetList />
      <FindPlanet />
      <CreatePlanet />

      <hr />
      <h2>Subscription</h2>
      <PlanetSubscription />
    </div>
  );
}

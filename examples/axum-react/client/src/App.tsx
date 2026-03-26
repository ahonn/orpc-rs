import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { orpc, client } from "./rpc";
import type { Planet } from "./rpc";

function PingButton() {
  const [result, setResult] = useState<string | null>(null);

  async function handlePing() {
    const res: string = await client.ping();
    setResult(res);
  }

  return (
    <div style={{ marginBottom: 24 }}>
      <h2>Ping</h2>
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
      <h2>Planets</h2>
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
      <h2>Find Planet</h2>
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
      {result && (
        <pre>{JSON.stringify(result, null, 2)}</pre>
      )}
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
      // Refresh the planet list after successful creation
      queryClient.invalidateQueries({ queryKey: orpc.planet.list.key() });
    } catch (err: any) {
      setError(err?.message ?? "Unknown error");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div style={{ marginBottom: 24 }}>
      <h2>Create Planet (Mutation)</h2>
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
      {result && (
        <pre>Created: {JSON.stringify(result, null, 2)}</pre>
      )}
      {error && <p style={{ color: "red" }}>{error}</p>}
    </div>
  );
}

export default function App() {
  return (
    <div style={{ maxWidth: 720, margin: "0 auto", padding: 24, fontFamily: "system-ui" }}>
      <h1>orpc-rs + React Example</h1>
      <p style={{ color: "#666" }}>
        React frontend talking to a Rust axum server via the oRPC RPC wire protocol.
      </p>
      <hr />
      <PingButton />
      <PlanetList />
      <FindPlanet />
      <CreatePlanet />
    </div>
  );
}

import { Boxes, Plus } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router-dom";
import { InstanceCard } from "../components/InstanceCard";
import { NewInstanceDialog } from "../components/NewInstanceDialog";
import { Button, EmptyState, Page } from "../components/ui";
import { useAccounts } from "../stores/accounts";
import { useInstances } from "../stores/instances";

export function Home() {
  const instances = useInstances((s) => s.instances);
  const loaded = useInstances((s) => s.loaded);
  const accounts = useAccounts((s) => s.accounts);
  const active = accounts.accounts.find((a) => a.uuid === accounts.active);
  const [creating, setCreating] = useState(false);

  // Already ordered most recently played first.
  const recent = instances.slice(0, 3);

  return (
    <Page title={active ? `Welcome back, ${active.username}` : "Welcome to Glint"}>
      {loaded && instances.length === 0 ? (
        <EmptyState
          icon={<Boxes size={36} strokeWidth={1.5} />}
          title="Create your first instance"
          action={
            <Button variant="primary" icon={<Plus size={16} />} onClick={() => setCreating(true)}>
              New instance
            </Button>
          }
        >
          An instance is its own copy of the game: a Minecraft version, mods and settings that
          don't affect your other instances.
        </EmptyState>
      ) : (
        <section className="flex flex-col gap-3">
          <div className="flex items-baseline justify-between">
            <h2 className="text-[13px] font-semibold tracking-wide text-muted uppercase">
              Jump back in
            </h2>
            <Link to="/instances" className="text-[13px] text-muted hover:text-accent">
              All instances
            </Link>
          </div>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-3">
            {recent.map((instance) => (
              <InstanceCard key={instance.id} instance={instance} />
            ))}
          </div>
        </section>
      )}
      {creating && <NewInstanceDialog onClose={() => setCreating(false)} />}
    </Page>
  );
}

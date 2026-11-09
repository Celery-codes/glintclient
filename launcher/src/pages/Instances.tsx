import { Boxes, Plus } from "lucide-react";
import { useState } from "react";
import { InstanceCard } from "../components/InstanceCard";
import { NewInstanceDialog } from "../components/NewInstanceDialog";
import { Button, EmptyState, Page } from "../components/ui";
import { useInstances } from "../stores/instances";

export function Instances() {
  const instances = useInstances((s) => s.instances);
  const loaded = useInstances((s) => s.loaded);
  const [creating, setCreating] = useState(false);

  const newButton = (
    <Button variant="primary" icon={<Plus size={16} />} onClick={() => setCreating(true)}>
      New instance
    </Button>
  );

  return (
    <Page title="Instances" actions={instances.length > 0 && newButton}>
      {loaded && instances.length === 0 ? (
        <EmptyState
          icon={<Boxes size={36} strokeWidth={1.5} />}
          title="No instances yet"
          action={newButton}
        >
          Each instance has its own Minecraft version, mods, memory and Java settings.
        </EmptyState>
      ) : (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-3">
          {instances.map((instance) => (
            <InstanceCard key={instance.id} instance={instance} />
          ))}
        </div>
      )}
      {creating && <NewInstanceDialog onClose={() => setCreating(false)} />}
    </Page>
  );
}

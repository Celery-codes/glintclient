import { Puzzle, Shirt } from "lucide-react";
import { EmptyState, Page } from "../components/ui";

export function Mods() {
  return (
    <Page title="Mods">
      <EmptyState icon={<Puzzle size={36} strokeWidth={1.5} />} title="The mod browser isn't built yet">
        Searching and installing from Modrinth and CurseForge arrives in the next phase. Until
        then, drop mod files into an instance's mods folder.
      </EmptyState>
    </Page>
  );
}

export function Skins() {
  return (
    <Page title="Skins">
      <EmptyState icon={<Shirt size={36} strokeWidth={1.5} />} title="The skin manager isn't built yet">
        Changing your skin and cape from the launcher comes in a later phase.
      </EmptyState>
    </Page>
  );
}

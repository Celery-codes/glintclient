import { Shirt } from "lucide-react";
import { EmptyState, Page } from "../components/ui";

export function Skins() {
  return (
    <Page title="Skins">
      <EmptyState icon={<Shirt size={36} strokeWidth={1.5} />} title="The skin manager isn't built yet">
        Changing your skin and cape from the launcher comes in a later phase.
      </EmptyState>
    </Page>
  );
}

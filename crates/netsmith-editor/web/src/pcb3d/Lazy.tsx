// The 3D view as the PCB view mounts it: its chunk (three.js and the view) is fetched from the
// binary the first time 3D opens, and the view shows a loading state until then (ADR 0105).

import type { FunctionComponent } from 'preact';
import { useEffect, useState } from 'preact/hooks';

import { EmptyState } from '../components/ui';
import { ViewLoading } from '../components/ViewStatus';
import { load3d } from './mode';

let loaded: FunctionComponent<{ board: string }> | null = null;

export function Board3d({ board }: { board: string }) {
  const [View, setView] = useState(() => loaded);
  const [failed, setFailed] = useState<string | null>(null);
  useEffect(() => {
    if (View) return;
    load3d()
      .then((m) => setView(() => (loaded = m.View3d)))
      .catch((e) => setFailed(e instanceof Error ? e.message : String(e)));
  }, []);
  if (failed) return <EmptyState icon="error" tone="bad" title="The 3D view could not be loaded" help={failed} />;
  if (!View) return <ViewLoading what="Loading the 3D view" />;
  return <View board={board} />;
}

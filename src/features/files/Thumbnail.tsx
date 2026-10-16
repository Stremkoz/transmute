import { useEffect, useState } from "react";
import type { TransmuteApi } from "../../api/client";
import { useApi, useSession } from "../../state/sessionContext";
import styles from "./FilesList.module.css";

/**
 * Thumbnails already asked for, per backend, keyed by inventory version and
 * entry: a row scrolled away and back doesn't ask twice, and a rescan
 * (which renumbers entries) starts afresh. `null`: none could be made.
 */
const asked = new WeakMap<TransmuteApi, Map<string, Promise<string | null>>>();

function thumbnailUrl(api: TransmuteApi, key: string, entry: number): Promise<string | null> {
  let cache = asked.get(api);
  if (!cache) {
    cache = new Map();
    asked.set(api, cache);
  }
  let url = cache.get(key);
  if (!url) {
    url = api.thumbnail(entry).then(
      (thumbnail) => api.previewUrl(thumbnail),
      () => null,
    );
    cache.set(key, url);
  }
  return url;
}

/** A small preview for one file's row; mounted only for rows in view. */
export function Thumbnail({ entry }: { entry: number }) {
  const api = useApi();
  const version = useSession((state) => state.inventoryVersion);
  const key = `${version}:${entry}`;
  const [shown, setShown] = useState<{ key: string; url: string | null } | null>(null);

  useEffect(() => {
    let current = true;
    void thumbnailUrl(api, key, entry).then((url) => current && setShown({ key, url }));
    return () => {
      current = false;
    };
  }, [api, key, entry]);

  const url = shown?.key === key ? shown.url : undefined;
  return (
    <span className={styles.thumb} aria-hidden="true" data-missing={url === null || undefined}>
      {url && <img src={url} alt="" draggable={false} />}
    </span>
  );
}

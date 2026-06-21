const DB_NAME = 'code-explorer-work-document-sources';
const STORE_NAME = 'sources';
const DB_VERSION = 1;

interface StoredWorkDocumentSource {
  id: string;
  sourceMarkdown: string;
  savedAt: number;
}

function getIndexedDb(): IDBFactory | null {
  return typeof indexedDB === 'undefined' ? null : indexedDB;
}

function openDb(): Promise<IDBDatabase | null> {
  const factory = getIndexedDb();
  if (!factory) return Promise.resolve(null);

  return new Promise((resolve, reject) => {
    const request = factory.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME, { keyPath: 'id' });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function withStore<T>(
  mode: IDBTransactionMode,
  run: (store: IDBObjectStore) => IDBRequest<T>
): Promise<T | null> {
  return openDb().then(
    (db) =>
      new Promise((resolve, reject) => {
        if (!db) {
          resolve(null);
          return;
        }
        const tx = db.transaction(STORE_NAME, mode);
        const store = tx.objectStore(STORE_NAME);
        const request = run(store);
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
        tx.oncomplete = () => db.close();
        tx.onerror = () => {
          db.close();
          reject(tx.error);
        };
      })
  );
}

export async function saveWorkDocumentSource(
  id: string,
  sourceMarkdown: string | undefined
): Promise<void> {
  if (!sourceMarkdown) return;
  const record: StoredWorkDocumentSource = {
    id,
    sourceMarkdown,
    savedAt: Date.now(),
  };
  await withStore('readwrite', (store) => store.put(record));
}

export async function loadWorkDocumentSource(id: string): Promise<string | null> {
  const record = await withStore<StoredWorkDocumentSource>('readonly', (store) => store.get(id));
  return record?.sourceMarkdown ?? null;
}

export async function deleteWorkDocumentSource(id: string): Promise<void> {
  await withStore('readwrite', (store) => store.delete(id));
}

CREATE TABLE DbSet (
                Id                                        TEXT NOT NULL CONSTRAINT "PK_DbSet" PRIMARY KEY,
                InstallDate                               TEXT,
                InstallDirectory                          TEXT,
                InstallVersion                            TEXT,
                InstallVersionName                        TEXT,
                Installed                                 INTEGER NOT NULL,
                LastUpdated                               TEXT,
                ProductAsin                               TEXT,
                ProductTitle                              TEXT,
                ManifestSignature                         TEXT,
                ManifestSignatureKeyId                    TEXT
              );

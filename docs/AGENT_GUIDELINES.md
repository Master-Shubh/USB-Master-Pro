# USB Master Pro — Developer & Engineering Invariant Guidelines

**Document Version**: 1.0.0  
**Classification**: Engineering Governance & Architecture Mandates  
**Author**: Antigravity (AI CTO & Commander)  
**Applicability**: All Software Engineers, Systems Architects, and Subagents  

---

## 1. Core Engineering Philosophy

USB Master Pro operates at the delicate intersection of Windows kernel-mode drivers, hardware bus I/O, asynchronous networking, and biometric privacy. In this domain, minor oversights cause catastrophic failures: blue screens of death (BSOD), system deadlocks, corrupted smart cards, or biometric data leaks.

Every engineer contributing to this repository MUST strictly observe the following mandatory engineering laws.

---

## 2. The Seven Sacred Engineering Invariants

```
+----------------------------------------------------------------------------------------------------+
|                                THE SEVEN SACRED ENGINEERING INVARIANTS                             |
+----------------------------------------------------------------------------------------------------+
|                                                                                                    |
|  1. ZERO MOCKS IN PRODUCTION CODE                                                                  |
|     • Never introduce mock objects, synthetic dummy data, or simulated delay loops into            |
|       production code paths (src/). Real kernel handles and network sockets must always be used.  |
|                                                                                                    |
|  2. THE ZERO-REBOOT INVARIANT                                                                      |
|     • Neither installation, runtime updating, nor uninstallation may EVER prompt the user         |
|       to restart their Windows operating system. All drivers must attach and detach dynamically.   |
|                                                                                                    |
|  3. NON-IDEMPOTENT RETRY PROHIBITION                                                              |
|     • Bulk OUT data transfers and Control OUT setup requests MUST NEVER be retransmitted          |
|       speculatively on socket timeout. Blind retries corrupt print jobs and lock smart card PINs.  |
|                                                                                                    |
|  4. ZERO UNVALIDATED BUFFER ALLOCATIONS                                                            |
|     • All network buffer lengths must be strictly validated against MAX_URB_BUFFER_SIZE (16 MB).   |
|       Zero unbounded allocations or naked pointer arithmetic are permitted.                        |
|                                                                                                    |
|  5. THE SELF-EXCLUSION INVARIANT                                                                   |
|     • anyusb_filter.sys must explicitly refuse to attach to synthetic devices created by           |
|       anyusb_vhci.sys (DEVPKEY_Device_EnumeratorName == 'LOGOS_VUSB'). Accidental capture = BSOD.  |
|                                                                                                    |
|  6. SURGICAL CHUNK EDITS ONLY                                                                      |
|     • Never rewrite an entire source file when applying patches. Use targeted contiguous chunk     |
|       replacements preserving untouched symbols, imports, and architectural comments.              |
|                                                                                                    |
|  7. ZERO HARDCODED NETWORK FALLBACKS                                                               |
|     • URLs, signaling broker endpoints, STUN/TURN clusters, and port assignments must be dynamic   |
|       and configurable via command-line flags or environment variables. No hardcoded defaults.     |
|                                                                                                    |
+----------------------------------------------------------------------------------------------------+
```

---

## 3. Detailed Invariant Explanations & Enforcement

### 3.1 Law 1: Zero Mocks in Production (`src/`)
- **Rule**: Code inside `core/src/` must interact with real operating system APIs, true hardware enumerators (`cfgmgr32.dll`), and genuine network sockets.
- **Testing Exception**: Unit and integration test suites in `core/tests/` may synthesize URB streams to simulate biometric scanners or network jitter, but production binaries must contain zero mock branches.

### 3.2 Law 2: Zero-Reboot Architecture
- **Rule**: Drivers must be designed for dynamic PnP staging and detachment.
- **Implementation**:
  - Use `pnputil.exe /add-driver <inf> /install` during installation.
  - Call `IoInvalidateDeviceRelations(DeviceObject, BusRelations)` to trigger live PnP re-enumeration.
  - During uninstallation, invoke `pnputil.exe /remove-device "root\anyusb_vhci"` followed by `/delete-driver <inf> /uninstall /force`.
  - Inno Setup scripts must configure `RestartIfNeededByRun=no`.

### 3.3 Law 3: Non-Idempotent Retry Prohibition
- **Rule**: USB is a stateful hardware bus. A dropped network packet does not mean the USB peripheral did not receive the transaction.
- **Implementation**:
  - Only idempotent reads (`FLAG_RD_SERVICE_OPTIMIZED` Bulk IN endpoints) may participate in speculative prefetch pools.
  - If a connection drops during a Bulk OUT or Control OUT transfer, the session must cleanly abort, log an error, and surface a retry prompt to the operator. Never silently replay the write.

### 3.4 Law 4: Zero Unvalidated Buffer Allocations
- **Rule**: Any buffer length received over IPC or network sockets must be rigorously verified before allocating memory.
- **Implementation**:
  ```rust
  pub const MAX_URB_BUFFER_SIZE: u32 = 16 * 1024 * 1024; // 16 MB Hard Ceiling
  
  if header.buffer_length > MAX_URB_BUFFER_SIZE || header.actual_length > MAX_URB_BUFFER_SIZE {
      return Err("Buffer length exceeds safe operating ceiling");
  }
  ```

### 3.5 Law 5: Self-Exclusion Invariant (Anti-BSOD Loopback)
- **Rule**: If a single PC acts as both Sender and Receiver, `anyusb_filter.sys` must verify that the target device is not a synthetic PDO generated by `anyusb_vhci.sys`.
- **Implementation**:
  In `anyusb_filter.sys`'s `EvtDeviceAdd`:
  ```c
  WDF_DEVICE_PROPERTY_DATA propertyData;
  WDF_DEVICE_PROPERTY_DATA_INIT(&propertyData, &DEVPKEY_Device_EnumeratorName);
  
  WCHAR enumeratorName[64];
  status = WdfDeviceQueryPropertyEx(Device, &propertyData, NonPagedPoolNx, ...);
  if (NT_SUCCESS(status) && wcscmp(enumeratorName, L"LOGOS_VUSB") == 0) {
      // Synthetic device created by anyusb_vhci.sys! REFUSE ATTACHMENT!
      return STATUS_NOT_SUPPORTED;
  }
  ```

### 3.6 Law 6: Surgical Chunk Edits Only
- **Rule**: When editing existing source code files, never overwrite the whole file. Always use targeted replacements (`replace_file_content`), preserving untouched symbols, imports, and comments.

### 3.7 Law 7: Zero Hardcoded Network Fallbacks
- **Rule**: Network endpoints must never be permanently baked into code without configuration overrides.
- **Implementation**: Broker URLs, STUN/TURN relays, and bind ports must be read from environment variables (`USB_MASTER_BROKER_URL`) or CLI flags (`--broker-url`), allowing seamless field migration across staging and enterprise deployments.

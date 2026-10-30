/**
 * USB Master Pro — Virtual USB Host Controller & Root Bus Driver (anyusb_vhci.sys)
 * Implementation of KMDF 1.33 Bus Enumerator & Child PDO Instantiation
 */

#include <initguid.h>
#include "vhci.h"

// Prototypes
DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD AnyUsbVhci_EvtDeviceAdd;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL AnyUsbVhci_EvtIoDeviceControl;
EVT_WDF_IO_QUEUE_IO_INTERNAL_DEVICE_CONTROL AnyUsbVhci_EvtIoInternalDeviceControl;
EVT_WDF_DEVICE_PROCESS_QUERY_INTERFACE_REQUEST AnyUsb_EvtDeviceProcessQueryInterfaceRequest;
EVT_WDF_REQUEST_CANCEL AnyUsbVhci_EvtPendingUrbCancel;

// Monotonically increasing 64-bit sequence counter (prevents ABA slot reuse races)
static volatile LONG64 g_VhciSequenceCounter = 1;

NTSTATUS
DriverEntry(
    _In_ PDRIVER_OBJECT  DriverObject,
    _In_ PUNICODE_STRING RegistryPath
)
{
    WDF_DRIVER_CONFIG config;
    NTSTATUS status;

    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "USB Master Pro: Initializing VHCI Bus Driver...\n"));

    WDF_DRIVER_CONFIG_INIT(&config, AnyUsbVhci_EvtDeviceAdd);
    status = WdfDriverCreate(DriverObject, RegistryPath, WDF_NO_OBJECT_ATTRIBUTES, &config, WDF_NO_HANDLE);

    return status;
}

NTSTATUS
AnyUsbVhci_EvtDeviceAdd(
    _In_    WDFDRIVER       Driver,
    _Inout_ PWDFDEVICE_INIT DeviceInit
)
{
    UNREFERENCED_PARAMETER(Driver);
    NTSTATUS status;
    WDF_OBJECT_ATTRIBUTES deviceAttributes;
    WDFDEVICE wdfDevice;
    PFDO_DEVICE_DATA fdoData;
    WDF_IO_QUEUE_CONFIG queueConfig;

    // Set Bus Device Characteristics
    WdfDeviceInitSetDeviceType(DeviceInit, FILE_DEVICE_BUS_EXTENDER);
    WdfDeviceInitSetExclusive(DeviceInit, FALSE);

    // Security Hardening: Restrict IOCTL access strictly to SYSTEM and Administrators (SDDL_DEVOBJ_SYS_ALL_ADM_ALL)
    DECLARE_CONST_UNICODE_STRING(sddlString, L"D:P(A;;GA;;;SY)(A;;GA;;;BA)");
    status = WdfDeviceInitAssignSDDLString(DeviceInit, &sddlString);
    if (!NT_SUCCESS(status)) {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
            "USB Master Pro: WdfDeviceInitAssignSDDLString failed: 0x%08X\n", status));
        return status;
    }

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&deviceAttributes, FDO_DEVICE_DATA);

    status = WdfDeviceCreate(&DeviceInit, &deviceAttributes, &wdfDevice);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    // Create Symbolic Link so user-mode service/daemon can open \\.\AnyUsbVhci
    DECLARE_CONST_UNICODE_STRING(symbolicLinkName, L"\\DosDevices\\AnyUsbVhci");
    status = WdfDeviceCreateSymbolicLink(wdfDevice, &symbolicLinkName);
    if (!NT_SUCCESS(status)) {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
            "USB Master Pro: WdfDeviceCreateSymbolicLink failed: 0x%08X\n", status));
        return status;
    }

    // Register standard PnP Device Interface GUID for bus enumerators
    status = WdfDeviceCreateDeviceInterface(wdfDevice, &GUID_DEVINTERFACE_ANYUSB_VHCI, NULL);
    if (!NT_SUCCESS(status)) {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_WARNING_LEVEL,
            "USB Master Pro: WdfDeviceCreateDeviceInterface failed: 0x%08X\n", status));
        // Non-fatal if symbolic link succeeded
    }

    fdoData = GetFdoData(wdfDevice);
    fdoData->WdfDevice = wdfDevice;
    fdoData->AttachedDeviceCount = 0;
    InitializeListHead(&fdoData->ChildPdoList);

    status = WdfSpinLockCreate(WDF_NO_OBJECT_ATTRIBUTES, &fdoData->DeviceListLock);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    status = WdfSpinLockCreate(WDF_NO_OBJECT_ATTRIBUTES, &fdoData->PendingUrbLock);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    RtlZeroMemory(fdoData->PendingRequests, sizeof(fdoData->PendingRequests));

    // Default I/O Queue for IOCTLs from USB Master Pro User-Mode Daemon
    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&queueConfig, WdfIoQueueDispatchParallel);
    queueConfig.EvtIoDeviceControl = AnyUsbVhci_EvtIoDeviceControl;

    status = WdfIoQueueCreate(wdfDevice, &queueConfig, WDF_NO_OBJECT_ATTRIBUTES, &fdoData->IoctlQueue);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    // Queue for holding pending URBs submitted by Guest OS Drivers
    WDF_IO_QUEUE_CONFIG_INIT(&queueConfig, WdfIoQueueDispatchManual);
    status = WdfIoQueueCreate(wdfDevice, &queueConfig, WDF_NO_OBJECT_ATTRIBUTES, &fdoData->PendingUrbQueue);

    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "USB Master Pro: VHCI Bus Root Device Created Successfully.\n"));
    return status;
}

// ---------------------------------------------------------------------------
// URB Transfer Buffer Helper (Virtual Buffer or MDL System Mapping)
// ---------------------------------------------------------------------------
static PVOID
AnyUsb_GetTransferBuffer(
    _In_opt_ PVOID TransferBuffer,
    _In_opt_ PMDL  TransferBufferMDL
)
{
    if (TransferBuffer != NULL) {
        return TransferBuffer;
    }
    if (TransferBufferMDL != NULL) {
        return MmGetSystemAddressForMdlSafe(TransferBufferMDL, NormalPagePriority);
    }
    return NULL;
}

// ---------------------------------------------------------------------------
// USB Bus Interface (USB_BUS_INTERFACE_USBDI_GUID) Implementations
// Required for WinUSB.sys and usbccgp.sys (Composite Driver) to bind cleanly
// ---------------------------------------------------------------------------

static VOID
NTAPI
AnyUsb_BusInterfaceReference(
    _In_opt_ PVOID BusContext
)
{
    UNREFERENCED_PARAMETER(BusContext);
}

static VOID
NTAPI
AnyUsb_BusInterfaceDereference(
    _In_opt_ PVOID BusContext
)
{
    UNREFERENCED_PARAMETER(BusContext);
}

static VOID
USB_BUSIFFN
AnyUsb_GetUSBDIVersion(
    _In_opt_  PVOID                       BusContext,
    _Out_opt_ PUSBD_VERSION_INFORMATION  VersionInformation,
    _Out_opt_ PULONG                      HcdCapabilities
)
{
    if (VersionInformation != NULL) {
        PPDO_DEVICE_DATA pdoData = (PPDO_DEVICE_DATA)BusContext;
        VersionInformation->USBDI_Version = USB_BUSIF_USBDI_VERSION_3;
        // Dynamically report SuperSpeed (USB 3.0) capability if child PDO is SuperSpeed
        if (pdoData != NULL && pdoData->Speed >= 4) {
            VersionInformation->Supported_USB_Version = 0x0300;
        } else {
            VersionInformation->Supported_USB_Version = 0x0200;
        }
    }

    if (HcdCapabilities != NULL) {
        *HcdCapabilities = USB_HCD_CAPS_SUPPORTS_RT_THREADS;
    }
}

static NTSTATUS
USB_BUSIFFN
AnyUsb_QueryBusTime(
    _In_opt_  PVOID   BusContext,
    _Out_opt_ PULONG  CurrentFrame
)
{
    UNREFERENCED_PARAMETER(BusContext);

    if (CurrentFrame != NULL) {
        *CurrentFrame = 0;
    }

    return STATUS_SUCCESS;
}

static NTSTATUS
USB_BUSIFFN
AnyUsb_SubmitIsoOutUrb(
    _In_opt_ PVOID BusContext,
    _In_opt_ PURB  Urb
)
{
    UNREFERENCED_PARAMETER(BusContext);
    UNREFERENCED_PARAMETER(Urb);

    return STATUS_NOT_SUPPORTED;
}

static NTSTATUS
USB_BUSIFFN
AnyUsb_QueryBusInformation(
    _In_opt_    PVOID   BusContext,
    _In_        ULONG   Level,
    _Inout_     PVOID   BusInformationBuffer,
    _Out_       PULONG  BusInformationBufferLength,
    _Out_opt_   PULONG  BusInformationActualLength
)
{
    UNREFERENCED_PARAMETER(BusContext);

    if (BusInformationActualLength != NULL) {
        *BusInformationActualLength = 0;
    }

    if (Level == 0) {
        if (BusInformationBufferLength == NULL ||
            *BusInformationBufferLength < sizeof(USB_BUS_INFORMATION_LEVEL_0)) {
            if (BusInformationActualLength != NULL) {
                *BusInformationActualLength = sizeof(USB_BUS_INFORMATION_LEVEL_0);
            }
            return STATUS_BUFFER_TOO_SMALL;
        }

        PUSB_BUS_INFORMATION_LEVEL_0 info = (PUSB_BUS_INFORMATION_LEVEL_0)BusInformationBuffer;
        info->TotalBandwidth = 480000;    // 480 Mbps
        info->ConsumedBandwidth = 10000;  // Minimal allocated
        *BusInformationBufferLength = sizeof(USB_BUS_INFORMATION_LEVEL_0);
        if (BusInformationActualLength != NULL) {
            *BusInformationActualLength = sizeof(USB_BUS_INFORMATION_LEVEL_0);
        }
        return STATUS_SUCCESS;
    }

    if (Level == 1) {
        if (BusInformationBufferLength == NULL ||
            *BusInformationBufferLength < sizeof(USB_BUS_INFORMATION_LEVEL_1)) {
            if (BusInformationActualLength != NULL) {
                *BusInformationActualLength = sizeof(USB_BUS_INFORMATION_LEVEL_1);
            }
            return STATUS_BUFFER_TOO_SMALL;
        }

        PUSB_BUS_INFORMATION_LEVEL_1 info = (PUSB_BUS_INFORMATION_LEVEL_1)BusInformationBuffer;
        info->TotalBandwidth = 480000;
        info->ConsumedBandwidth = 10000;
        info->ControllerNameLength = 0;
        info->ControllerNameUnicodeString[0] = L'\0';
        *BusInformationBufferLength = sizeof(USB_BUS_INFORMATION_LEVEL_1);
        if (BusInformationActualLength != NULL) {
            *BusInformationActualLength = sizeof(USB_BUS_INFORMATION_LEVEL_1);
        }
        return STATUS_SUCCESS;
    }

    return STATUS_NOT_SUPPORTED;
}

static BOOLEAN
USB_BUSIFFN
AnyUsb_IsDeviceHighSpeed(
    _In_opt_ PVOID BusContext
)
{
    if (BusContext != NULL) {
        PPDO_DEVICE_DATA pdoData = (PPDO_DEVICE_DATA)BusContext;
        // Speed: 1 = Low, 2 = Full, 3 = High, 4 = SuperSpeed
        if (pdoData->Speed >= 3) {
            return TRUE;
        }
    }
    return FALSE;
}

static NTSTATUS
USB_BUSIFFN
AnyUsb_EnumLogEntry(
    _In_opt_ PVOID BusContext,
    _In_     ULONG DriverTag,
    _In_     ULONG EnumTag,
    _In_     ULONG P1,
    _In_     ULONG P2
)
{
    UNREFERENCED_PARAMETER(BusContext);
    UNREFERENCED_PARAMETER(DriverTag);
    UNREFERENCED_PARAMETER(EnumTag);
    UNREFERENCED_PARAMETER(P1);
    UNREFERENCED_PARAMETER(P2);

    return STATUS_SUCCESS;
}

NTSTATUS
AnyUsb_EvtDeviceProcessQueryInterfaceRequest(
    _In_    WDFDEVICE Device,
    _In_    LPGUID    InterfaceType,
    _Inout_ PINTERFACE ExposedInterface,
    _Inout_opt_ PVOID ExposedInterfaceSpecificData
)
{
    UNREFERENCED_PARAMETER(InterfaceType);
    UNREFERENCED_PARAMETER(ExposedInterfaceSpecificData);

    PPDO_DEVICE_DATA pdoData = GetPdoData(Device);
    if (pdoData == NULL || ExposedInterface == NULL) {
        return STATUS_INVALID_PARAMETER;
    }

    ExposedInterface->Context = pdoData;
    ExposedInterface->InterfaceReference = AnyUsb_BusInterfaceReference;
    ExposedInterface->InterfaceDereference = AnyUsb_BusInterfaceDereference;

    if (ExposedInterface->Size >= sizeof(USB_BUS_INTERFACE_USBDI_V0)) {
        PUSB_BUS_INTERFACE_USBDI_V0 v0 = (PUSB_BUS_INTERFACE_USBDI_V0)ExposedInterface;
        v0->BusContext = pdoData;
        v0->GetUSBDIVersion = AnyUsb_GetUSBDIVersion;
        v0->QueryBusTime = AnyUsb_QueryBusTime;
        // Setting SubmitIsoOutUrb to NULL forces usbaudio.sys to dispatch isochronous transfers via standard WDM IRPs
        v0->SubmitIsoOutUrb = NULL;
        v0->QueryBusInformation = AnyUsb_QueryBusInformation;
    }

    if (ExposedInterface->Size >= sizeof(USB_BUS_INTERFACE_USBDI_V1)) {
        PUSB_BUS_INTERFACE_USBDI_V1 v1 = (PUSB_BUS_INTERFACE_USBDI_V1)ExposedInterface;
        v1->IsDeviceHighSpeed = AnyUsb_IsDeviceHighSpeed;
    }

    if (ExposedInterface->Size >= sizeof(USB_BUS_INTERFACE_USBDI_V2)) {
        PUSB_BUS_INTERFACE_USBDI_V2 v2 = (PUSB_BUS_INTERFACE_USBDI_V2)ExposedInterface;
        v2->EnumLogEntry = AnyUsb_EnumLogEntry;
    }

    return STATUS_SUCCESS;
}

VOID
AnyUsbVhci_EvtPendingUrbCancel(
    _In_ WDFREQUEST Request
)
{
    WDFDEVICE dev = WdfIoQueueGetDevice(WdfRequestGetIoQueue(Request));
    PFDO_DEVICE_DATA fdoData = NULL;

    if (dev != NULL) {
        PPDO_DEVICE_DATA pdoData = GetPdoData(dev);
        if (pdoData != NULL && pdoData->ParentFdo != NULL) {
            fdoData = GetFdoData(pdoData->ParentFdo);
        }
    }

    if (fdoData != NULL) {
        WdfSpinLockAcquire(fdoData->PendingUrbLock);
        for (ULONG i = 0; i < ANYUSB_MAX_PENDING_URBS; i++) {
            if (fdoData->PendingRequests[i].Request == Request) {
                fdoData->PendingRequests[i].Request = NULL;
                fdoData->PendingRequests[i].SequenceId = 0;
                fdoData->PendingRequests[i].DeviceId = 0;
                break;
            }
        }
        WdfSpinLockRelease(fdoData->PendingUrbLock);
    }

    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
        "USB Master Pro: Pending guest URB request %p cancelled\n", Request));
    WdfRequestComplete(Request, STATUS_CANCELLED);
}

VOID
AnyUsbVhci_EvtIoDeviceControl(
    _In_ WDFQUEUE   Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t     OutputBufferLength,
    _In_ size_t     InputBufferLength,
    _In_ ULONG      IoControlCode
)
{
    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(InputBufferLength);

    WDFDEVICE device = WdfIoQueueGetDevice(Queue);
    PFDO_DEVICE_DATA fdoData = GetFdoData(device);
    NTSTATUS status = STATUS_SUCCESS;
    size_t bytesReturned = 0;

    switch (IoControlCode) {
        case IOCTL_ANYUSB_VHCI_INIT_BUS:
            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "USB Master Pro: Daemon initialized bus session.\n"));
            status = STATUS_SUCCESS;
            break;

        case IOCTL_ANYUSB_VHCI_ATTACH_DEVICE: {
            PANYUSB_ATTACH_REQUEST attachReq = NULL;
            status = WdfRequestRetrieveInputBuffer(Request, sizeof(ANYUSB_ATTACH_REQUEST), (PVOID*)&attachReq, NULL);
            if (!NT_SUCCESS(status) || attachReq == NULL) {
                status = STATUS_INVALID_PARAMETER;
                break;
            }

            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
                "USB Master Pro: Synthesizing Child PDO for Remote Device [VID:0x%04X PID:0x%04X DeviceId:%lu]...\n",
                attachReq->VendorId, attachReq->ProductId, attachReq->DeviceId));

            // Allocate child PDO initialization structure
            PWDFDEVICE_INIT pdoInit = WdfPdoInitAllocate(device);
            if (pdoInit == NULL) {
                status = STATUS_INSUFFICIENT_RESOURCES;
                break;
            }

            WdfDeviceInitSetDeviceType(pdoInit, FILE_DEVICE_UNKNOWN);

            // Construct PnP Hardware, Compatible, and Instance IDs
            DECLARE_CONST_UNICODE_STRING(compatId, L"USB\\COMPOSITE");
            DECLARE_UNICODE_STRING_SIZE(hwId, 128);
            DECLARE_UNICODE_STRING_SIZE(devId, 64);
            DECLARE_UNICODE_STRING_SIZE(instId, 64);
            DECLARE_UNICODE_STRING_SIZE(devText, 128);
            DECLARE_UNICODE_STRING_SIZE(locText, 64);

            RtlUnicodeStringPrintf(&hwId, L"USB\\VID_%04X&PID_%04X&REV_%04X",
                attachReq->VendorId, attachReq->ProductId, attachReq->BcdDevice);
            RtlUnicodeStringPrintf(&devId, L"USB\\VID_%04X&PID_%04X",
                attachReq->VendorId, attachReq->ProductId);
            RtlUnicodeStringPrintf(&instId, L"%08X", attachReq->DeviceId);

            if (attachReq->ProductDescription[0] != L'\0') {
                RtlUnicodeStringPrintf(&devText, L"%ws", attachReq->ProductDescription);
            } else {
                RtlUnicodeStringPrintf(&devText, L"AnyUSB Virtual USB Device (VID_%04X PID_%04X)",
                    attachReq->VendorId, attachReq->ProductId);
            }
            RtlUnicodeStringPrintf(&locText, L"AnyUSB Virtual Bus Port %lu", attachReq->DeviceId);

            status = WdfPdoInitAssignDeviceID(pdoInit, &devId);
            if (!NT_SUCCESS(status)) {
                WdfDeviceInitFree(pdoInit);
                break;
            }

            status = WdfPdoInitAssignInstanceID(pdoInit, &instId);
            if (!NT_SUCCESS(status)) {
                WdfDeviceInitFree(pdoInit);
                break;
            }

            status = WdfPdoInitAddHardwareID(pdoInit, &hwId);
            if (!NT_SUCCESS(status)) {
                WdfDeviceInitFree(pdoInit);
                break;
            }

            status = WdfPdoInitAddCompatibleID(pdoInit, &compatId);
            if (!NT_SUCCESS(status)) {
                WdfDeviceInitFree(pdoInit);
                break;
            }

            status = WdfPdoInitAddDeviceText(pdoInit, &devText, &locText, 0x409);
            if (!NT_SUCCESS(status)) {
                WdfDeviceInitFree(pdoInit);
                break;
            }
            WdfPdoInitSetDefaultLocale(pdoInit, 0x409);

            // Configure Child PDO Context
            WDF_OBJECT_ATTRIBUTES pdoAttributes;
            WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&pdoAttributes, PDO_DEVICE_DATA);

            WDFDEVICE childDevice = NULL;
            status = WdfDeviceCreate(&pdoInit, &pdoAttributes, &childDevice);
            if (!NT_SUCCESS(status)) {
                KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                    "USB Master Pro: WdfDeviceCreate for Child PDO failed: 0x%08X\n", status));
                break;
            }

            PPDO_DEVICE_DATA pdoData = GetPdoData(childDevice);
            pdoData->WdfDevice = childDevice;
            pdoData->ParentFdo = device;
            pdoData->DeviceId = attachReq->DeviceId;
            pdoData->VendorId = attachReq->VendorId;
            pdoData->ProductId = attachReq->ProductId;
            pdoData->BcdDevice = attachReq->BcdDevice;
            pdoData->Speed = attachReq->Speed;

            // Configure Child PDO Internal I/O Queue to intercept guest driver URBs
            WDF_IO_QUEUE_CONFIG childQueueConfig;
            WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&childQueueConfig, WdfIoQueueDispatchParallel);
            childQueueConfig.EvtIoInternalDeviceControl = AnyUsbVhci_EvtIoInternalDeviceControl;

            WDFQUEUE childQueue = NULL;
            status = WdfIoQueueCreate(childDevice, &childQueueConfig, WDF_NO_OBJECT_ATTRIBUTES, &childQueue);
            if (!NT_SUCCESS(status)) {
                KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                    "USB Master Pro: WdfIoQueueCreate for Child PDO internal queue failed: 0x%08X\n", status));
                break;
            }

            // Configure USB_BUS_INTERFACE_USBDI_GUID query interface for WinUSB and composite drivers
            WDF_QUERY_INTERFACE_CONFIG qiConfig;
            RtlZeroMemory(&pdoData->BusInterface, sizeof(USB_BUS_INTERFACE_USBDI_V2));
            pdoData->BusInterface.Size = sizeof(USB_BUS_INTERFACE_USBDI_V2);
            pdoData->BusInterface.Version = USB_BUSIF_USBDI_VERSION_2;
            pdoData->BusInterface.BusContext = pdoData;
            pdoData->BusInterface.InterfaceReference = AnyUsb_BusInterfaceReference;
            pdoData->BusInterface.InterfaceDereference = AnyUsb_BusInterfaceDereference;
            pdoData->BusInterface.GetUSBDIVersion = AnyUsb_GetUSBDIVersion;
            pdoData->BusInterface.QueryBusTime = AnyUsb_QueryBusTime;
            // Setting SubmitIsoOutUrb to NULL forces usbaudio.sys to dispatch isochronous transfers via standard WDM IRPs
            pdoData->BusInterface.SubmitIsoOutUrb = NULL;
            pdoData->BusInterface.QueryBusInformation = AnyUsb_QueryBusInformation;
            pdoData->BusInterface.IsDeviceHighSpeed = AnyUsb_IsDeviceHighSpeed;
            pdoData->BusInterface.EnumLogEntry = AnyUsb_EnumLogEntry;

            WDF_QUERY_INTERFACE_CONFIG_INIT(
                &qiConfig,
                (PINTERFACE)&pdoData->BusInterface,
                &USB_BUS_INTERFACE_USBDI_GUID,
                AnyUsb_EvtDeviceProcessQueryInterfaceRequest
            );

            status = WdfDeviceAddQueryInterface(childDevice, &qiConfig);
            if (!NT_SUCCESS(status)) {
                KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                    "USB Master Pro: WdfDeviceAddQueryInterface for USB_BUS_INTERFACE_USBDI failed: 0x%08X\n", status));
                break;
            }

            // Add Child PDO to Virtual Bus FDO
            status = WdfFdoAddStaticChild(device, childDevice);
            if (!NT_SUCCESS(status)) {
                KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                    "USB Master Pro: WdfFdoAddStaticChild failed: 0x%08X\n", status));
                break;
            }

            // Link into tracked child list
            WdfSpinLockAcquire(fdoData->DeviceListLock);
            InsertTailList(&fdoData->ChildPdoList, &pdoData->Link);
            fdoData->AttachedDeviceCount++;
            WdfSpinLockRelease(fdoData->DeviceListLock);

            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
                "USB Master Pro: Child PDO successfully instantiated. Bus Device Count: %lu\n",
                fdoData->AttachedDeviceCount));

            status = STATUS_SUCCESS;
            break;
        }

        case IOCTL_ANYUSB_VHCI_DETACH_DEVICE: {
            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "USB Master Pro: Detaching remote virtual device.\n"));

            ULONG targetDeviceId = 0;
            PULONG pInputDeviceId = NULL;
            if (NT_SUCCESS(WdfRequestRetrieveInputBuffer(Request, sizeof(ULONG), (PVOID*)&pInputDeviceId, NULL)) && pInputDeviceId != NULL) {
                targetDeviceId = *pInputDeviceId;
            }

            WDFDEVICE targetChildDevice = NULL;
            PPDO_DEVICE_DATA targetPdoData = NULL;

            WdfSpinLockAcquire(fdoData->DeviceListLock);
            for (PLIST_ENTRY entry = fdoData->ChildPdoList.Flink;
                 entry != &fdoData->ChildPdoList;
                 entry = entry->Flink) {
                PPDO_DEVICE_DATA pdoData = CONTAINING_RECORD(entry, PDO_DEVICE_DATA, Link);
                if (targetDeviceId == 0 || pdoData->DeviceId == targetDeviceId) {
                    targetChildDevice = pdoData->WdfDevice;
                    targetPdoData = pdoData;
                    RemoveEntryList(&pdoData->Link);
                    InitializeListHead(&pdoData->Link);
                    if (fdoData->AttachedDeviceCount > 0) {
                        fdoData->AttachedDeviceCount--;
                    }
                    break;
                }
            }
            WdfSpinLockRelease(fdoData->DeviceListLock);

            if (targetChildDevice != NULL) {
                // Cancel any in-flight pending requests belonging to this child device
                WdfSpinLockAcquire(fdoData->PendingUrbLock);
                for (ULONG i = 0; i < ANYUSB_MAX_PENDING_URBS; i++) {
                    WDFREQUEST pendingReq = fdoData->PendingRequests[i].Request;
                    if (pendingReq != NULL) {
                        WDFDEVICE reqDev = WdfIoQueueGetDevice(WdfRequestGetIoQueue(pendingReq));
                        if (reqDev == targetChildDevice) {
                            fdoData->PendingRequests[i].Request = NULL;
                            fdoData->PendingRequests[i].SequenceId = 0;
                            fdoData->PendingRequests[i].DeviceId = 0;
                            if (NT_SUCCESS(WdfRequestUnmarkCancelable(pendingReq))) {
                                WdfRequestComplete(pendingReq, STATUS_CANCELLED);
                            }
                        }
                    }
                }
                WdfSpinLockRelease(fdoData->PendingUrbLock);

                // Mark child PDO missing (WdfPdoMarkMissing automatically invalidates bus relations)
                status = WdfPdoMarkMissing(targetChildDevice);
                if (NT_SUCCESS(status)) {
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
                        "USB Master Pro: Child PDO [DeviceId:%lu] marked missing and relations invalidated. Remaining: %lu\n",
                        targetPdoData->DeviceId, fdoData->AttachedDeviceCount));
                    status = STATUS_SUCCESS;
                } else {
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                        "USB Master Pro: WdfPdoMarkMissing failed: 0x%08X\n", status));
                }
                UNREFERENCED_PARAMETER(targetPdoData);
            } else {
                KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_WARNING_LEVEL,
                    "USB Master Pro: Detach target device ID %lu not found on bus\n", targetDeviceId));
                status = STATUS_NOT_FOUND;
            }
            break;
        }

        case IOCTL_ANYUSB_VHCI_FETCH_PENDING_URB: {
            PVOID outputBuffer = NULL;
            size_t outBufSize = 0;
            status = WdfRequestRetrieveOutputBuffer(Request, sizeof(ANYUSB_URB_MESSAGE), &outputBuffer, &outBufSize);
            if (!NT_SUCCESS(status)) {
                break;
            }

            WDFREQUEST pendingGuestRequest = NULL;
            status = WdfIoQueueRetrieveNextRequest(fdoData->PendingUrbQueue, &pendingGuestRequest);
            if (NT_SUCCESS(status) && pendingGuestRequest != NULL) {
                // Obtain guest WDM IRP and URB
                PIRP irp = WdfRequestWdmGetIrp(pendingGuestRequest);
                PIO_STACK_LOCATION irpSp = (irp != NULL) ? IoGetCurrentIrpStackLocation(irp) : NULL;
                PURB urb = (irpSp != NULL) ? (PURB)irpSp->Parameters.Others.Argument1 : NULL;

                // Validate urb != NULL. If NULL, fail request with STATUS_INVALID_PARAMETER
                if (urb == NULL) {
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                        "USB Master Pro: Pending guest request has null URB\n"));
                    WdfRequestComplete(pendingGuestRequest, STATUS_INVALID_PARAMETER);
                    bytesReturned = 0;
                    status = STATUS_INVALID_PARAMETER;
                    break;
                }

                PANYUSB_URB_MESSAGE msg = (PANYUSB_URB_MESSAGE)outputBuffer;
                RtlZeroMemory(msg, sizeof(ANYUSB_URB_MESSAGE));

                // Populate DeviceId from the request's originating queue/device if available
                WDFQUEUE reqQueue = WdfRequestGetIoQueue(pendingGuestRequest);
                if (reqQueue != NULL) {
                    WDFDEVICE childDev = WdfIoQueueGetDevice(reqQueue);
                    if (childDev != NULL) {
                        PPDO_DEVICE_DATA pdoData = GetPdoData(childDev);
                        if (pdoData != NULL) {
                            msg->DeviceId = pdoData->DeviceId;
                        }
                    }
                }

                // Inspect urb->UrbHeader.Function and extract buffer / setup data FIRST (before marking cancelable)
                USHORT urbFunc = urb->UrbHeader.Function;
                if (urbFunc == URB_FUNCTION_CONTROL_TRANSFER) {
                    msg->TransferType = 0;
                    msg->EndpointAddress = 0x00;
                    RtlCopyMemory(msg->SetupPacket, urb->UrbControlTransfer.SetupPacket, sizeof(msg->SetupPacket));
                    msg->TransferBufferLength = urb->UrbControlTransfer.TransferBufferLength;

                    // Reject if requested buffer exceeds single-URB maximum (64 KB)
                    if (msg->TransferBufferLength > sizeof(msg->Data)) {
                        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                            "USB Master Pro VHCI: Control transfer size %lu exceeds 64 KB limit. Request rejected.\n",
                            msg->TransferBufferLength));
                        WdfRequestComplete(pendingGuestRequest, STATUS_BUFFER_OVERFLOW);
                        bytesReturned = 0;
                        status = STATUS_BUFFER_OVERFLOW;
                        break;
                    }

                    // If OUT transfer, copy guest payload into msg->Data
                    if (!(msg->SetupPacket[0] & 0x80) && msg->TransferBufferLength > 0) {
                        PVOID buf = AnyUsb_GetTransferBuffer(urb->UrbControlTransfer.TransferBuffer,
                                                             urb->UrbControlTransfer.TransferBufferMDL);
                        if (buf != NULL) {
                            RtlCopyMemory(msg->Data, buf, msg->TransferBufferLength);
                        }
                    }
                } else if (urbFunc == URB_FUNCTION_CONTROL_TRANSFER_EX) {
                    msg->TransferType = 0;
                    msg->EndpointAddress = 0x00;
                    RtlCopyMemory(msg->SetupPacket, urb->UrbControlTransferEx.SetupPacket, sizeof(msg->SetupPacket));
                    msg->TransferBufferLength = urb->UrbControlTransferEx.TransferBufferLength;

                    // Reject if requested buffer exceeds single-URB maximum (64 KB)
                    if (msg->TransferBufferLength > sizeof(msg->Data)) {
                        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                            "USB Master Pro VHCI: Control Ex transfer size %lu exceeds 64 KB limit. Request rejected.\n",
                            msg->TransferBufferLength));
                        WdfRequestComplete(pendingGuestRequest, STATUS_BUFFER_OVERFLOW);
                        bytesReturned = 0;
                        status = STATUS_BUFFER_OVERFLOW;
                        break;
                    }

                    // If OUT transfer, copy guest payload into msg->Data
                    if (!(msg->SetupPacket[0] & 0x80) && msg->TransferBufferLength > 0) {
                        PVOID buf = AnyUsb_GetTransferBuffer(urb->UrbControlTransferEx.TransferBuffer,
                                                             urb->UrbControlTransferEx.TransferBufferMDL);
                        if (buf != NULL) {
                            RtlCopyMemory(msg->Data, buf, msg->TransferBufferLength);
                        }
                    }
                } else if (urbFunc == URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER) {
                    msg->TransferType = 2;
                    msg->EndpointAddress = (UCHAR)((ULONG_PTR)urb->UrbBulkOrInterruptTransfer.PipeHandle & 0xFF);
                    msg->TransferBufferLength = urb->UrbBulkOrInterruptTransfer.TransferBufferLength;

                    // Reject if requested buffer exceeds single-URB maximum (64 KB) to prevent silent truncation
                    if (msg->TransferBufferLength > sizeof(msg->Data)) {
                        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                            "USB Master Pro VHCI: Bulk transfer size %lu exceeds 64 KB limit. Rejecting without truncation.\n",
                            msg->TransferBufferLength));
                        WdfRequestComplete(pendingGuestRequest, STATUS_BUFFER_OVERFLOW);
                        bytesReturned = 0;
                        status = STATUS_BUFFER_OVERFLOW;
                        break;
                    }

                    // If OUT transfer, copy guest payload into msg->Data
                    if (!(urb->UrbBulkOrInterruptTransfer.TransferFlags & USBD_TRANSFER_DIRECTION_IN) && msg->TransferBufferLength > 0) {
                        PVOID buf = AnyUsb_GetTransferBuffer(urb->UrbBulkOrInterruptTransfer.TransferBuffer,
                                                             urb->UrbBulkOrInterruptTransfer.TransferBufferMDL);
                        if (buf != NULL) {
                            RtlCopyMemory(msg->Data, buf, msg->TransferBufferLength);
                        }
                    }
                } else if (urbFunc == URB_FUNCTION_ISOCH_TRANSFER) {
                    msg->TransferType = 1;
                    msg->EndpointAddress = (UCHAR)((ULONG_PTR)urb->UrbIsochronousTransfer.PipeHandle & 0xFF);
                    msg->TransferBufferLength = urb->UrbIsochronousTransfer.TransferBufferLength;

                    // Reject if requested buffer exceeds single-URB maximum (64 KB)
                    if (msg->TransferBufferLength > sizeof(msg->Data)) {
                        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                            "USB Master Pro VHCI: Isochronous transfer size %lu exceeds 64 KB limit. Rejecting.\n",
                            msg->TransferBufferLength));
                        WdfRequestComplete(pendingGuestRequest, STATUS_BUFFER_OVERFLOW);
                        bytesReturned = 0;
                        status = STATUS_BUFFER_OVERFLOW;
                        break;
                    }

                    if (!(urb->UrbIsochronousTransfer.TransferFlags & USBD_TRANSFER_DIRECTION_IN) && msg->TransferBufferLength > 0) {
                        PVOID buf = AnyUsb_GetTransferBuffer(urb->UrbIsochronousTransfer.TransferBuffer,
                                                             urb->UrbIsochronousTransfer.TransferBufferMDL);
                        if (buf != NULL) {
                            RtlCopyMemory(msg->Data, buf, msg->TransferBufferLength);
                        }
                    }
                } else {
                    // Reject unsupported URB function cleanly instead of returning bogus zeroed success
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_WARNING_LEVEL,
                        "USB Master Pro VHCI: Unsupported URB function 0x%04X rejected with NOT_SUPPORTED\n", urbFunc));
                    urb->UrbHeader.Status = USBD_STATUS_NOT_SUPPORTED;
                    WdfRequestComplete(pendingGuestRequest, STATUS_NOT_SUPPORTED);
                    bytesReturned = 0;
                    status = STATUS_NOT_SUPPORTED;
                    break;
                }

                // Allocate monotonic 64-bit sequence ID (prevents ABA slot reuse races)
                ULONG64 seqId = (ULONG64)InterlockedIncrement64(&g_VhciSequenceCounter);
                msg->SequenceId = seqId;

                // Find a free slot in the bounded sequence table and mark cancelable at the very end
                ULONG assignedSlot = ANYUSB_MAX_PENDING_URBS;
                WdfSpinLockAcquire(fdoData->PendingUrbLock);
                for (ULONG i = 0; i < ANYUSB_MAX_PENDING_URBS; i++) {
                    if (fdoData->PendingRequests[i].Request == NULL) {
                        assignedSlot = i;
                        break;
                    }
                }

                if (assignedSlot < ANYUSB_MAX_PENDING_URBS) {
                    NTSTATUS markStatus = WdfRequestMarkCancelableEx(pendingGuestRequest, AnyUsbVhci_EvtPendingUrbCancel);
                    if (NT_SUCCESS(markStatus)) {
                        fdoData->PendingRequests[assignedSlot].Request = pendingGuestRequest;
                        fdoData->PendingRequests[assignedSlot].SequenceId = seqId;
                        fdoData->PendingRequests[assignedSlot].DeviceId = msg->DeviceId;
                    } else {
                        // Request was cancelled while attempting to mark
                        assignedSlot = ANYUSB_MAX_PENDING_URBS;
                        WdfRequestComplete(pendingGuestRequest, markStatus);
                        WdfSpinLockRelease(fdoData->PendingUrbLock);
                        bytesReturned = 0;
                        status = markStatus;
                        break;
                    }
                }
                WdfSpinLockRelease(fdoData->PendingUrbLock);

                if (assignedSlot == ANYUSB_MAX_PENDING_URBS) {
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                        "USB Master Pro: Pending URB sequence table is full (max %d)\n", ANYUSB_MAX_PENDING_URBS));
                    WdfRequestComplete(pendingGuestRequest, STATUS_INSUFFICIENT_RESOURCES);
                    bytesReturned = 0;
                    status = STATUS_INSUFFICIENT_RESOURCES;
                    break;
                }

                bytesReturned = sizeof(ANYUSB_URB_MESSAGE);
                status = STATUS_SUCCESS;
            } else {
                bytesReturned = 0;
                status = STATUS_NO_MORE_ENTRIES;
            }
            break;
        }

        case IOCTL_ANYUSB_VHCI_COMPLETE_URB: {
            PANYUSB_COMPLETE_REQUEST compReq = NULL;
            status = WdfRequestRetrieveInputBuffer(Request, sizeof(ANYUSB_COMPLETE_REQUEST), (PVOID*)&compReq, NULL);
            if (NT_SUCCESS(status) && compReq != NULL) {
                KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
                    "USB Master Pro: Completing URB Seq %llu (Status: 0x%08X, Len: %lu)\n",
                    compReq->SequenceId, compReq->Status, compReq->ActualLength));

                ULONG64 seqId = compReq->SequenceId;
                WDFREQUEST targetRequest = NULL;

                // Explicitly reject completion if ActualLength exceeds buffer capacity to prevent truncation
                if (compReq->ActualLength > sizeof(compReq->Data)) {
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                        "USB Master Pro VHCI: compReq ActualLength %lu exceeds buffer size %lu. Rejecting without truncation.\n",
                        compReq->ActualLength, (ULONG)sizeof(compReq->Data)));
                    status = STATUS_BUFFER_OVERFLOW;
                    break;
                }

                WdfSpinLockAcquire(fdoData->PendingUrbLock);
                for (ULONG i = 0; i < ANYUSB_MAX_PENDING_URBS; i++) {
                    if (fdoData->PendingRequests[i].Request != NULL &&
                        fdoData->PendingRequests[i].SequenceId == seqId) {
                        
                        // Verify DeviceId binding to prevent cross-device memory corruption
                        if (compReq->DeviceId != 0 && fdoData->PendingRequests[i].DeviceId != 0 &&
                            compReq->DeviceId != fdoData->PendingRequests[i].DeviceId) {
                            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                                "USB Master Pro: DeviceId mismatch for Seq %llu (Expected %lu, Got %lu)\n",
                                seqId, fdoData->PendingRequests[i].DeviceId, compReq->DeviceId));
                            break;
                        }

                        WDFREQUEST candidate = fdoData->PendingRequests[i].Request;
                        NTSTATUS unmarkStatus = WdfRequestUnmarkCancelable(candidate);
                        if (NT_SUCCESS(unmarkStatus)) {
                            targetRequest = candidate;
                            fdoData->PendingRequests[i].Request = NULL;
                            fdoData->PendingRequests[i].SequenceId = 0;
                            fdoData->PendingRequests[i].DeviceId = 0;
                        } else {
                            // Cancel routine already owns completion; drop slot cleanly
                            fdoData->PendingRequests[i].Request = NULL;
                            fdoData->PendingRequests[i].SequenceId = 0;
                            fdoData->PendingRequests[i].DeviceId = 0;
                        }
                        break;
                    }
                }
                WdfSpinLockRelease(fdoData->PendingUrbLock);

                if (targetRequest == NULL) {
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_WARNING_LEVEL,
                        "USB Master Pro: Stale, duplicate, or cancelled SequenceId %llu dropped\n", seqId));
                    status = STATUS_NOT_FOUND;
                    break;
                }

                // Retrieve guest WDM IRP and URB from targetRequest
                PIRP irp = WdfRequestWdmGetIrp(targetRequest);
                if (irp != NULL) {
                    PIO_STACK_LOCATION irpSp = IoGetCurrentIrpStackLocation(irp);
                    PURB urb = (irpSp != NULL) ? (PURB)irpSp->Parameters.Others.Argument1 : NULL;
                    if (urb != NULL) {
                        urb->UrbHeader.Status = compReq->Status;

                        if (urb->UrbHeader.Function == URB_FUNCTION_CONTROL_TRANSFER) {
                            urb->UrbControlTransfer.TransferBufferLength = compReq->ActualLength;
                            // If IN transfer and compReq->ActualLength > 0: copy compReq->Data into guest buffer
                            if ((urb->UrbControlTransfer.SetupPacket[0] & 0x80) && compReq->ActualLength > 0) {
                                ULONG copyLen = compReq->ActualLength;
                                if (copyLen > sizeof(compReq->Data)) {
                                    copyLen = sizeof(compReq->Data);
                                }
                                PVOID dst = AnyUsb_GetTransferBuffer(urb->UrbControlTransfer.TransferBuffer,
                                                                     urb->UrbControlTransfer.TransferBufferMDL);
                                if (dst != NULL) {
                                    RtlCopyMemory(dst, compReq->Data, copyLen);
                                }
                            }
                        } else if (urb->UrbHeader.Function == URB_FUNCTION_CONTROL_TRANSFER_EX) {
                            urb->UrbControlTransferEx.TransferBufferLength = compReq->ActualLength;
                            if ((urb->UrbControlTransferEx.SetupPacket[0] & 0x80) && compReq->ActualLength > 0) {
                                ULONG copyLen = compReq->ActualLength;
                                if (copyLen > sizeof(compReq->Data)) {
                                    copyLen = sizeof(compReq->Data);
                                }
                                PVOID dst = AnyUsb_GetTransferBuffer(urb->UrbControlTransferEx.TransferBuffer,
                                                                     urb->UrbControlTransferEx.TransferBufferMDL);
                                if (dst != NULL) {
                                    RtlCopyMemory(dst, compReq->Data, copyLen);
                                }
                            }
                        } else if (urb->UrbHeader.Function == URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER) {
                            urb->UrbBulkOrInterruptTransfer.TransferBufferLength = compReq->ActualLength;
                            // If IN transfer and compReq->ActualLength > 0: copy compReq->Data into guest buffer
                            if ((urb->UrbBulkOrInterruptTransfer.TransferFlags & USBD_TRANSFER_DIRECTION_IN) && compReq->ActualLength > 0) {
                                ULONG copyLen = compReq->ActualLength;
                                if (copyLen > sizeof(compReq->Data)) {
                                    copyLen = sizeof(compReq->Data);
                                }
                                PVOID dst = AnyUsb_GetTransferBuffer(urb->UrbBulkOrInterruptTransfer.TransferBuffer,
                                                                     urb->UrbBulkOrInterruptTransfer.TransferBufferMDL);
                                if (dst != NULL) {
                                    RtlCopyMemory(dst, compReq->Data, copyLen);
                                }
                            }
                        } else if (urb->UrbHeader.Function == URB_FUNCTION_ISOCH_TRANSFER) {
                            urb->UrbIsochronousTransfer.TransferBufferLength = compReq->ActualLength;
                            if ((urb->UrbIsochronousTransfer.TransferFlags & USBD_TRANSFER_DIRECTION_IN) && compReq->ActualLength > 0) {
                                ULONG copyLen = compReq->ActualLength;
                                if (copyLen > sizeof(compReq->Data)) {
                                    copyLen = sizeof(compReq->Data);
                                }
                                PVOID dst = AnyUsb_GetTransferBuffer(urb->UrbIsochronousTransfer.TransferBuffer,
                                                                     urb->UrbIsochronousTransfer.TransferBufferMDL);
                                if (dst != NULL) {
                                    RtlCopyMemory(dst, compReq->Data, copyLen);
                                }
                            }
                        } else if (urb->UrbHeader.Function == URB_FUNCTION_SELECT_CONFIGURATION) {
                            PUSBD_INTERFACE_INFORMATION iface = &urb->UrbSelectConfiguration.Interface;
                            for (ULONG i = 0; i < iface->NumberOfPipes; i++) {
                                iface->Pipes[i].PipeHandle = (USBD_PIPE_HANDLE)(ULONG_PTR)iface->Pipes[i].EndpointAddress;
                            }
                        } else if (urb->UrbHeader.Function == URB_FUNCTION_SELECT_INTERFACE) {
                            PUSBD_INTERFACE_INFORMATION iface = &urb->UrbSelectInterface.Interface;
                            for (ULONG i = 0; i < iface->NumberOfPipes; i++) {
                                iface->Pipes[i].PipeHandle = (USBD_PIPE_HANDLE)(ULONG_PTR)iface->Pipes[i].EndpointAddress;
                            }
                        }
                    }
                }

                // Complete targetRequest with compReq->Status and compReq->ActualLength
                WdfRequestCompleteWithInformation(targetRequest, compReq->Status, compReq->ActualLength);
                bytesReturned = 0;
                status = STATUS_SUCCESS;
            }
            break;
        }

        default:
            status = STATUS_INVALID_DEVICE_REQUEST;
            break;
    }

    WdfRequestCompleteWithInformation(Request, status, bytesReturned);
}

VOID
AnyUsbVhci_EvtIoInternalDeviceControl(
    _In_ WDFQUEUE   Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t     OutputBufferLength,
    _In_ size_t     InputBufferLength,
    _In_ ULONG      IoControlCode
)
{
    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(InputBufferLength);

    WDFDEVICE childDevice = WdfIoQueueGetDevice(Queue);
    PPDO_DEVICE_DATA pdoData = GetPdoData(childDevice);
    if (pdoData == NULL || pdoData->ParentFdo == NULL) {
        WdfRequestComplete(Request, STATUS_INVALID_DEVICE_STATE);
        return;
    }

    PFDO_DEVICE_DATA fdoData = GetFdoData(pdoData->ParentFdo);
    if (fdoData == NULL || fdoData->PendingUrbQueue == NULL) {
        WdfRequestComplete(Request, STATUS_INVALID_DEVICE_STATE);
        return;
    }

    if (IoControlCode == IOCTL_INTERNAL_USB_SUBMIT_URB) {
        // Enqueue guest driver URB into FDO PendingUrbQueue for daemon processing
        NTSTATUS status = WdfRequestForwardToIoQueue(Request, fdoData->PendingUrbQueue);
        if (!NT_SUCCESS(status)) {
            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                "USB Master Pro: WdfRequestForwardToIoQueue failed: 0x%08X\n", status));
            WdfRequestComplete(Request, status);
        }
    } else {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
            "USB Master Pro: Unhandled internal IOCTL 0x%08X on child PDO\n", IoControlCode));
        WdfRequestComplete(Request, STATUS_NOT_SUPPORTED);
    }
}

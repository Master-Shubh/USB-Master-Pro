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

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&deviceAttributes, FDO_DEVICE_DATA);

    status = WdfDeviceCreate(&DeviceInit, &deviceAttributes, &wdfDevice);
    if (!NT_SUCCESS(status)) {
        return status;
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
    UNREFERENCED_PARAMETER(BusContext);

    if (VersionInformation != NULL) {
        VersionInformation->USBDI_Version = USB_BUSIF_USBDI_VERSION_3;
        VersionInformation->Supported_USB_Version = 0x0200; // USB 2.0 High-Speed
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
        v0->SubmitIsoOutUrb = AnyUsb_SubmitIsoOutUrb;
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
            DECLARE_CONST_UNICODE_STRING(compatId, L"USB\\COMPOSITE\0");
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
            pdoData->BusInterface.SubmitIsoOutUrb = AnyUsb_SubmitIsoOutUrb;
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
            status = WdfFdoAddChild(device, childDevice);
            if (!NT_SUCCESS(status)) {
                KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                    "USB Master Pro: WdfFdoAddChild failed: 0x%08X\n", status));
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
                    WDFREQUEST pendingReq = fdoData->PendingRequests[i];
                    if (pendingReq != NULL) {
                        WDFQUEUE reqQ = WdfRequestGetIoQueue(pendingReq);
                        if (reqQ != NULL && WdfIoQueueGetDevice(reqQ) == targetChildDevice) {
                            fdoData->PendingRequests[i] = NULL;
                            WdfRequestComplete(pendingReq, STATUS_CANCELLED);
                        }
                    }
                }
                WdfSpinLockRelease(fdoData->PendingUrbLock);

                // Mark child PDO missing and request child relations invalidation
                status = WdfPdoMarkMissing(targetChildDevice);
                if (NT_SUCCESS(status)) {
                    WDFCHILDLIST childList = WdfFdoGetDefaultChildList(device);
                    if (childList != NULL) {
                        WdfChildListRequestChildRelations(childList);
                    }
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

                // Find a free slot in the bounded sequence table
                ULONG assignedSlot = ANYUSB_MAX_PENDING_URBS;
                WdfSpinLockAcquire(fdoData->PendingUrbLock);
                for (ULONG i = 0; i < ANYUSB_MAX_PENDING_URBS; i++) {
                    if (fdoData->PendingRequests[i] == NULL) {
                        fdoData->PendingRequests[i] = pendingGuestRequest;
                        assignedSlot = i;
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

                PANYUSB_URB_MESSAGE msg = (PANYUSB_URB_MESSAGE)outputBuffer;
                RtlZeroMemory(msg, sizeof(ANYUSB_URB_MESSAGE));
                // Safe 1-based bounded sequence ID (never leaks raw kernel pointer)
                msg->SequenceId = (ULONG64)(assignedSlot + 1);

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

                // Inspect urb->UrbHeader.Function and extract buffer / setup data
                USHORT urbFunc = urb->UrbHeader.Function;
                if (urbFunc == URB_FUNCTION_CONTROL_TRANSFER) {
                    msg->TransferType = 0;
                    msg->EndpointAddress = 0x00;
                    RtlCopyMemory(msg->SetupPacket, urb->UrbControlTransfer.SetupPacket, sizeof(msg->SetupPacket));
                    msg->TransferBufferLength = urb->UrbControlTransfer.TransferBufferLength;

                    // If OUT transfer, copy guest payload into msg->Data
                    if (!(msg->SetupPacket[0] & 0x80)) {
                        ULONG copyLen = msg->TransferBufferLength;
                        if (copyLen > sizeof(msg->Data)) {
                            copyLen = sizeof(msg->Data);
                        }
                        if (copyLen > 0) {
                            PVOID buf = AnyUsb_GetTransferBuffer(urb->UrbControlTransfer.TransferBuffer,
                                                                 urb->UrbControlTransfer.TransferBufferMDL);
                            if (buf != NULL) {
                                RtlCopyMemory(msg->Data, buf, copyLen);
                            }
                        }
                    }
                } else if (urbFunc == URB_FUNCTION_CONTROL_TRANSFER_EX) {
                    msg->TransferType = 0;
                    msg->EndpointAddress = 0x00;
                    RtlCopyMemory(msg->SetupPacket, urb->UrbControlTransferEx.SetupPacket, sizeof(msg->SetupPacket));
                    msg->TransferBufferLength = urb->UrbControlTransferEx.TransferBufferLength;

                    // If OUT transfer, copy guest payload into msg->Data
                    if (!(msg->SetupPacket[0] & 0x80)) {
                        ULONG copyLen = msg->TransferBufferLength;
                        if (copyLen > sizeof(msg->Data)) {
                            copyLen = sizeof(msg->Data);
                        }
                        if (copyLen > 0) {
                            PVOID buf = AnyUsb_GetTransferBuffer(urb->UrbControlTransferEx.TransferBuffer,
                                                                 urb->UrbControlTransferEx.TransferBufferMDL);
                            if (buf != NULL) {
                                RtlCopyMemory(msg->Data, buf, copyLen);
                            }
                        }
                    }
                } else if (urbFunc == URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER) {
                    msg->TransferType = 2;
                    msg->EndpointAddress = (UCHAR)((ULONG_PTR)urb->UrbBulkOrInterruptTransfer.PipeHandle & 0xFF);
                    msg->TransferBufferLength = urb->UrbBulkOrInterruptTransfer.TransferBufferLength;

                    // If OUT transfer, copy guest payload into msg->Data
                    if (!(urb->UrbBulkOrInterruptTransfer.TransferFlags & USBD_TRANSFER_DIRECTION_IN)) {
                        ULONG copyLen = msg->TransferBufferLength;
                        if (copyLen > sizeof(msg->Data)) {
                            copyLen = sizeof(msg->Data);
                        }
                        if (copyLen > 0) {
                            PVOID buf = AnyUsb_GetTransferBuffer(urb->UrbBulkOrInterruptTransfer.TransferBuffer,
                                                                 urb->UrbBulkOrInterruptTransfer.TransferBufferMDL);
                            if (buf != NULL) {
                                RtlCopyMemory(msg->Data, buf, copyLen);
                            }
                        }
                    }
                } else if (urbFunc == URB_FUNCTION_ISOCH_TRANSFER) {
                    msg->TransferType = 1;
                    msg->EndpointAddress = (UCHAR)((ULONG_PTR)urb->UrbIsochronousTransfer.PipeHandle & 0xFF);
                    msg->TransferBufferLength = urb->UrbIsochronousTransfer.TransferBufferLength;

                    if (!(urb->UrbIsochronousTransfer.TransferFlags & USBD_TRANSFER_DIRECTION_IN)) {
                        ULONG copyLen = msg->TransferBufferLength;
                        if (copyLen > sizeof(msg->Data)) {
                            copyLen = sizeof(msg->Data);
                        }
                        if (copyLen > 0) {
                            PVOID buf = AnyUsb_GetTransferBuffer(urb->UrbIsochronousTransfer.TransferBuffer,
                                                                 urb->UrbIsochronousTransfer.TransferBufferMDL);
                            if (buf != NULL) {
                                RtlCopyMemory(msg->Data, buf, copyLen);
                            }
                        }
                    }
                } else {
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
                        "USB Master Pro: Pending URB function 0x%04X fetched\n", urbFunc));
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
                if (seqId < 1 || seqId > ANYUSB_MAX_PENDING_URBS) {
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                        "USB Master Pro: Invalid SequenceId %llu (out of bounds [1, %d])\n",
                        seqId, ANYUSB_MAX_PENDING_URBS));
                    status = STATUS_INVALID_PARAMETER;
                    break;
                }

                ULONG slot = (ULONG)(seqId - 1);
                WDFREQUEST targetRequest = NULL;

                WdfSpinLockAcquire(fdoData->PendingUrbLock);
                targetRequest = fdoData->PendingRequests[slot];
                fdoData->PendingRequests[slot] = NULL;
                WdfSpinLockRelease(fdoData->PendingUrbLock);

                if (targetRequest != NULL) {
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
                            }
                        }
                    }

                    // Complete targetRequest with compReq->Status and compReq->ActualLength
                    WdfRequestCompleteWithInformation(targetRequest, compReq->Status, compReq->ActualLength);
                    status = STATUS_SUCCESS;
                } else {
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_WARNING_LEVEL,
                        "USB Master Pro: SequenceId %llu has already been completed or cancelled\n", seqId));
                    status = STATUS_INVALID_PARAMETER;
                }
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

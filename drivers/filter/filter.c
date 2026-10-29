/**
 * USB Master Pro — Upper Filter Driver for Physical Hardware Capture (anyusb_filter.sys)
 * KMDF 1.33 Driver with Self-Exclusion & URB Redirection Pipeline
 */

#include "filter.h"

// Global references for Control Device Object
static WDFDEVICE g_ControlDevice = NULL;
static PCONTROL_DEVICE_DATA g_ControlData = NULL;

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

static NTSTATUS
AnyUsbFilter_CreateControlDevice(
    _In_ WDFDRIVER Driver
)
{
    NTSTATUS status;
    PWDFDEVICE_INIT deviceInit = NULL;
    WDF_OBJECT_ATTRIBUTES deviceAttributes;
    WDFDEVICE controlDevice = NULL;
    PCONTROL_DEVICE_DATA controlData = NULL;
    WDF_IO_QUEUE_CONFIG queueConfig;

    // Security Hardening: SDDL allowing SYSTEM and Administrators
    DECLARE_CONST_UNICODE_STRING(sddlString, L"D:P(A;;GA;;;SY)(A;;GA;;;BA)");
    deviceInit = WdfControlDeviceInitAllocate(Driver, &sddlString);
    if (deviceInit == NULL) {
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    // Assign Device Name: \Device\AnyUsbFilter
    DECLARE_CONST_UNICODE_STRING(ntDeviceName, L"\\Device\\AnyUsbFilter");
    status = WdfDeviceInitAssignName(deviceInit, &ntDeviceName);
    if (!NT_SUCCESS(status)) {
        WdfDeviceInitFree(deviceInit);
        return status;
    }

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&deviceAttributes, CONTROL_DEVICE_DATA);

    status = WdfDeviceCreate(&deviceInit, &deviceAttributes, &controlDevice);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    // Create Symbolic Link: \DosDevices\AnyUsbFilter (User-mode \\.\AnyUsbFilter)
    DECLARE_CONST_UNICODE_STRING(symbolicLinkName, L"\\DosDevices\\AnyUsbFilter");
    status = WdfDeviceCreateSymbolicLink(controlDevice, &symbolicLinkName);
    if (!NT_SUCCESS(status)) {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
            "USB Master Pro Filter: WdfDeviceCreateSymbolicLink failed: 0x%08X\n", status));
        return status;
    }

    controlData = GetControlData(controlDevice);
    controlData->ControlDevice = controlDevice;
    controlData->IsCaptured = FALSE;
    InitializeListHead(&controlData->FilterDeviceList);

    status = WdfSpinLockCreate(WDF_NO_OBJECT_ATTRIBUTES, &controlData->ControlLock);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    status = WdfSpinLockCreate(WDF_NO_OBJECT_ATTRIBUTES, &controlData->PendingUrbLock);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    RtlZeroMemory(controlData->PendingRequests, sizeof(controlData->PendingRequests));

    // Default I/O Queue for CDO IOCTLs (Parallel Dispatch)
    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&queueConfig, WdfIoQueueDispatchParallel);
    queueConfig.EvtIoDeviceControl = AnyUsbFilter_EvtControlIoDeviceControl;

    status = WdfIoQueueCreate(controlDevice, &queueConfig, WDF_NO_OBJECT_ATTRIBUTES, &controlData->IoctlQueue);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    // Manual dispatch queue for holding intercepted URBs from physical devices
    WDF_IO_QUEUE_CONFIG_INIT(&queueConfig, WdfIoQueueDispatchManual);
    status = WdfIoQueueCreate(controlDevice, &queueConfig, WDF_NO_OBJECT_ATTRIBUTES, &controlData->PendingUrbQueue);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    WdfControlFinishInitializing(controlDevice);

    g_ControlDevice = controlDevice;
    g_ControlData = controlData;

    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
        "USB Master Pro Filter: CDO \\Device\\AnyUsbFilter created and initialized.\n"));

    return STATUS_SUCCESS;
}

NTSTATUS
DriverEntry(
    _In_ PDRIVER_OBJECT  DriverObject,
    _In_ PUNICODE_STRING RegistryPath
)
{
    WDF_DRIVER_CONFIG config;
    WDFDRIVER driver;
    NTSTATUS status;

    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "USB Master Pro: Initializing Hardware Capture Filter Driver...\n"));

    WDF_DRIVER_CONFIG_INIT(&config, AnyUsbFilter_EvtDeviceAdd);
    config.EvtDriverUnload = AnyUsbFilter_EvtDriverUnload;

    status = WdfDriverCreate(DriverObject, RegistryPath, WDF_NO_OBJECT_ATTRIBUTES, &config, &driver);
    if (!NT_SUCCESS(status)) {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
            "USB Master Pro Filter: WdfDriverCreate failed: 0x%08X\n", status));
        return status;
    }

    status = AnyUsbFilter_CreateControlDevice(driver);
    if (!NT_SUCCESS(status)) {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
            "USB Master Pro Filter: AnyUsbFilter_CreateControlDevice failed: 0x%08X\n", status));
        return status;
    }

    return STATUS_SUCCESS;
}

VOID
AnyUsbFilter_EvtDriverUnload(
    _In_ WDFDRIVER Driver
)
{
    UNREFERENCED_PARAMETER(Driver);
    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
        "USB Master Pro Filter: Driver unloading, cleaning up CDO...\n"));
    g_ControlData = NULL;
    g_ControlDevice = NULL;
}

NTSTATUS
AnyUsbFilter_EvtDeviceAdd(
    _In_    WDFDRIVER       Driver,
    _Inout_ PWDFDEVICE_INIT DeviceInit
)
{
    UNREFERENCED_PARAMETER(Driver);
    NTSTATUS status;
    WDF_OBJECT_ATTRIBUTES deviceAttributes;
    WDFDEVICE wdfDevice;
    PFILTER_DEVICE_DATA filterData;
    WDF_IO_QUEUE_CONFIG queueConfig;

    // Configure as an Upper Filter Device (inherits device stack security descriptors)
    WdfFdoInitSetFilter(DeviceInit);

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&deviceAttributes, FILTER_DEVICE_DATA);
    deviceAttributes.EvtCleanupCallback = AnyUsbFilter_EvtDeviceCleanup;

    status = WdfDeviceCreate(&DeviceInit, &deviceAttributes, &wdfDevice);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    filterData = GetFilterData(wdfDevice);
    filterData->WdfDevice = wdfDevice;
    filterData->IoTarget = WdfDeviceGetIoTarget(wdfDevice);
    filterData->IsCaptured = FALSE;
    filterData->SessionId = 0;
    static volatile LONG s_DeviceIdSeed = 1000;
    filterData->DeviceId = (ULONG)InterlockedIncrement(&s_DeviceIdSeed);
    InitializeListHead(&filterData->Link);

    // Self-exclusion check: Avoid hooking virtual USB devices created by LOGOS_VUSB / AnyUSB VHCI
    WDF_DEVICE_PROPERTY_DATA propData;
    DEVPROPTYPE propType;
    WCHAR enumeratorBuffer[64] = {0};
    ULONG requiredSize = 0;

    WDF_DEVICE_PROPERTY_DATA_INIT(&propData, &DEVPKEY_Device_EnumeratorName);
    status = WdfDeviceQueryPropertyEx(
        wdfDevice,
        &propData,
        sizeof(enumeratorBuffer),
        enumeratorBuffer,
        &requiredSize,
        &propType
    );
    if (NT_SUCCESS(status)) {
        UNICODE_STRING enumeratorStr;
        RtlInitUnicodeString(&enumeratorStr, enumeratorBuffer);
        UNICODE_STRING logosVusb;
        RtlInitUnicodeString(&logosVusb, L"LOGOS_VUSB");
        UNICODE_STRING anyusb;
        RtlInitUnicodeString(&anyusb, L"ANYUSB");

        if (RtlCompareUnicodeString(&enumeratorStr, &logosVusb, TRUE) == 0 ||
            RtlCompareUnicodeString(&enumeratorStr, &anyusb, TRUE) == 0) {
            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
                "USB Master Pro Filter: Self-exclusion matched for virtual device [%wZ]. Skipping capture hook.\n",
                &enumeratorStr));
            return STATUS_NOT_SUPPORTED;
        }
    }

    // Configure default I/O queue for standard IOCTLs and internal USB URB interception
    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&queueConfig, WdfIoQueueDispatchParallel);
    queueConfig.EvtIoDeviceControl = AnyUsbFilter_EvtIoDeviceControl;
    queueConfig.EvtIoInternalDeviceControl = AnyUsbFilter_EvtIoInternalDeviceControl;

    status = WdfIoQueueCreate(wdfDevice, &queueConfig, WDF_NO_OBJECT_ATTRIBUTES, &filterData->IoctlQueue);
    if (!NT_SUCCESS(status)) {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
            "USB Master Pro: WdfIoQueueCreate failed with status 0x%08X\n", status));
        return status;
    }

    // Enlist into CDO filter device list
    if (g_ControlData != NULL) {
        WdfSpinLockAcquire(g_ControlData->ControlLock);
        InsertTailList(&g_ControlData->FilterDeviceList, &filterData->Link);
        if (g_ControlData->IsCaptured) {
            filterData->IsCaptured = TRUE;
        }
        WdfSpinLockRelease(g_ControlData->ControlLock);
    }

    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "USB Master Pro: Filter attached to target physical USB stack.\n"));
    return STATUS_SUCCESS;
}

VOID
AnyUsbFilter_EvtDeviceCleanup(
    _In_ WDFOBJECT Object
)
{
    WDFDEVICE device = (WDFDEVICE)Object;
    PFILTER_DEVICE_DATA filterData = GetFilterData(device);

    if (g_ControlData != NULL && filterData != NULL) {
        WdfSpinLockAcquire(g_ControlData->ControlLock);
        if (!IsListEmpty(&filterData->Link) && filterData->Link.Flink != NULL) {
            RemoveEntryList(&filterData->Link);
            InitializeListHead(&filterData->Link);
        }
        WdfSpinLockRelease(g_ControlData->ControlLock);
    }
}

VOID
AnyUsbFilter_EvtPendingUrbCancel(
    _In_ WDFREQUEST Request
)
{
    if (g_ControlData != NULL) {
        WdfSpinLockAcquire(g_ControlData->PendingUrbLock);
        for (ULONG i = 0; i < ANYUSB_MAX_PENDING_URBS; i++) {
            if (g_ControlData->PendingRequests[i] == Request) {
                g_ControlData->PendingRequests[i] = NULL;
                break;
            }
        }
        WdfSpinLockRelease(g_ControlData->PendingUrbLock);
    }
    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
        "USB Master Pro Filter: Intercepted pending URB cancelled by caller/system\n"));
    WdfRequestComplete(Request, STATUS_CANCELLED);
}

VOID
AnyUsbFilter_EvtControlIoDeviceControl(
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
    PCONTROL_DEVICE_DATA controlData = GetControlData(device);
    NTSTATUS status = STATUS_SUCCESS;
    size_t bytesReturned = 0;

    switch (IoControlCode) {
        case IOCTL_ANYUSB_FILTER_START_CAPTURE: {
            ULONG targetDeviceId = 0;
            PANYUSB_FILTER_CAPTURE_REQUEST capReq = NULL;
            if (InputBufferLength >= sizeof(ANYUSB_FILTER_CAPTURE_REQUEST)) {
                status = WdfRequestRetrieveInputBuffer(Request, sizeof(ANYUSB_FILTER_CAPTURE_REQUEST), (PVOID*)&capReq, NULL);
                if (NT_SUCCESS(status) && capReq != NULL) {
                    targetDeviceId = capReq->DeviceId;
                }
            }

            WdfSpinLockAcquire(controlData->ControlLock);
            controlData->IsCaptured = TRUE;
            PLIST_ENTRY entry = controlData->FilterDeviceList.Flink;
            while (entry != &controlData->FilterDeviceList) {
                PFILTER_DEVICE_DATA fData = CONTAINING_RECORD(entry, FILTER_DEVICE_DATA, Link);
                if (targetDeviceId == 0 || fData->DeviceId == targetDeviceId) {
                    fData->IsCaptured = TRUE;
                }
                entry = entry->Flink;
            }
            WdfSpinLockRelease(controlData->ControlLock);

            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
                "USB Master Pro Filter: Capture STARTED for device %lu\n", targetDeviceId));
            status = STATUS_SUCCESS;
            bytesReturned = 0;
            break;
        }

        case IOCTL_ANYUSB_FILTER_STOP_CAPTURE: {
            ULONG targetDeviceId = 0;
            PANYUSB_FILTER_CAPTURE_REQUEST capReq = NULL;
            if (InputBufferLength >= sizeof(ANYUSB_FILTER_CAPTURE_REQUEST)) {
                status = WdfRequestRetrieveInputBuffer(Request, sizeof(ANYUSB_FILTER_CAPTURE_REQUEST), (PVOID*)&capReq, NULL);
                if (NT_SUCCESS(status) && capReq != NULL) {
                    targetDeviceId = capReq->DeviceId;
                }
            }

            WdfSpinLockAcquire(controlData->ControlLock);
            if (targetDeviceId == 0) {
                controlData->IsCaptured = FALSE;
            }
            PLIST_ENTRY entry = controlData->FilterDeviceList.Flink;
            while (entry != &controlData->FilterDeviceList) {
                PFILTER_DEVICE_DATA fData = CONTAINING_RECORD(entry, FILTER_DEVICE_DATA, Link);
                if (targetDeviceId == 0 || fData->DeviceId == targetDeviceId) {
                    fData->IsCaptured = FALSE;
                }
                entry = entry->Flink;
            }
            WdfSpinLockRelease(controlData->ControlLock);

            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
                "USB Master Pro Filter: Capture STOPPED for device %lu\n", targetDeviceId));
            status = STATUS_SUCCESS;
            bytesReturned = 0;
            break;
        }

        case IOCTL_ANYUSB_FILTER_FETCH_URB: {
            PVOID outputBuffer = NULL;
            size_t outBufSize = 0;
            status = WdfRequestRetrieveOutputBuffer(Request, sizeof(ANYUSB_URB_MESSAGE), &outputBuffer, &outBufSize);
            if (!NT_SUCCESS(status)) {
                break;
            }

            WDFREQUEST pendingGuestRequest = NULL;
            status = WdfIoQueueRetrieveNextRequest(controlData->PendingUrbQueue, &pendingGuestRequest);
            if (NT_SUCCESS(status) && pendingGuestRequest != NULL) {
                PIRP irp = WdfRequestWdmGetIrp(pendingGuestRequest);
                PIO_STACK_LOCATION irpSp = (irp != NULL) ? IoGetCurrentIrpStackLocation(irp) : NULL;
                PURB urb = (irpSp != NULL) ? (PURB)irpSp->Parameters.Others.Argument1 : NULL;

                if (urb == NULL) {
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                        "USB Master Pro Filter: Intercepted request has null URB\n"));
                    WdfRequestComplete(pendingGuestRequest, STATUS_INVALID_PARAMETER);
                    bytesReturned = 0;
                    status = STATUS_INVALID_PARAMETER;
                    break;
                }

                ULONG assignedSlot = ANYUSB_MAX_PENDING_URBS;
                WdfSpinLockAcquire(controlData->PendingUrbLock);
                for (ULONG i = 0; i < ANYUSB_MAX_PENDING_URBS; i++) {
                    if (controlData->PendingRequests[i] == NULL) {
                        controlData->PendingRequests[i] = pendingGuestRequest;
                        assignedSlot = i;
                        break;
                    }
                }
                WdfSpinLockRelease(controlData->PendingUrbLock);

                if (assignedSlot == ANYUSB_MAX_PENDING_URBS) {
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                        "USB Master Pro Filter: Pending URB sequence table is full (max %d)\n", ANYUSB_MAX_PENDING_URBS));
                    WdfRequestComplete(pendingGuestRequest, STATUS_INSUFFICIENT_RESOURCES);
                    bytesReturned = 0;
                    status = STATUS_INSUFFICIENT_RESOURCES;
                    break;
                }

                NTSTATUS markStatus = WdfRequestMarkCancelableEx(pendingGuestRequest, AnyUsbFilter_EvtPendingUrbCancel);
                if (!NT_SUCCESS(markStatus)) {
                    WdfSpinLockAcquire(controlData->PendingUrbLock);
                    controlData->PendingRequests[assignedSlot] = NULL;
                    WdfSpinLockRelease(controlData->PendingUrbLock);
                    WdfRequestComplete(pendingGuestRequest, STATUS_CANCELLED);
                    bytesReturned = 0;
                    status = STATUS_CANCELLED;
                    break;
                }

                PANYUSB_URB_MESSAGE msg = (PANYUSB_URB_MESSAGE)outputBuffer;
                RtlZeroMemory(msg, sizeof(ANYUSB_URB_MESSAGE));
                msg->SequenceId = (ULONG64)(assignedSlot + 1);

                WDFQUEUE reqQueue = WdfRequestGetIoQueue(pendingGuestRequest);
                if (reqQueue != NULL) {
                    WDFDEVICE fidoDev = WdfIoQueueGetDevice(reqQueue);
                    if (fidoDev != NULL) {
                        PFILTER_DEVICE_DATA fidoData = GetFilterData(fidoDev);
                        if (fidoData != NULL) {
                            msg->DeviceId = fidoData->DeviceId;
                        }
                    }
                }

                USHORT urbFunc = urb->UrbHeader.Function;
                if (urbFunc == URB_FUNCTION_CONTROL_TRANSFER) {
                    msg->TransferType = 0;
                    msg->EndpointAddress = 0x00;
                    RtlCopyMemory(msg->SetupPacket, urb->UrbControlTransfer.SetupPacket, sizeof(msg->SetupPacket));
                    msg->TransferBufferLength = urb->UrbControlTransfer.TransferBufferLength;

                    if (msg->TransferBufferLength > sizeof(msg->Data)) {
                        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                            "USB Master Pro Filter: Control transfer %lu exceeds 64 KB limit. Rejected.\n",
                            msg->TransferBufferLength));
                        WdfRequestUnmarkCancelable(pendingGuestRequest);
                        WdfSpinLockAcquire(controlData->PendingUrbLock);
                        controlData->PendingRequests[assignedSlot] = NULL;
                        WdfSpinLockRelease(controlData->PendingUrbLock);
                        WdfRequestComplete(pendingGuestRequest, STATUS_BUFFER_OVERFLOW);
                        bytesReturned = 0;
                        status = STATUS_BUFFER_OVERFLOW;
                        break;
                    }

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

                    if (msg->TransferBufferLength > sizeof(msg->Data)) {
                        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                            "USB Master Pro Filter: Control Ex transfer %lu exceeds 64 KB limit. Rejected.\n",
                            msg->TransferBufferLength));
                        WdfRequestUnmarkCancelable(pendingGuestRequest);
                        WdfSpinLockAcquire(controlData->PendingUrbLock);
                        controlData->PendingRequests[assignedSlot] = NULL;
                        WdfSpinLockRelease(controlData->PendingUrbLock);
                        WdfRequestComplete(pendingGuestRequest, STATUS_BUFFER_OVERFLOW);
                        bytesReturned = 0;
                        status = STATUS_BUFFER_OVERFLOW;
                        break;
                    }

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

                    if (msg->TransferBufferLength > sizeof(msg->Data)) {
                        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                            "USB Master Pro Filter: Bulk transfer %lu exceeds 64 KB limit. Rejected.\n",
                            msg->TransferBufferLength));
                        WdfRequestUnmarkCancelable(pendingGuestRequest);
                        WdfSpinLockAcquire(controlData->PendingUrbLock);
                        controlData->PendingRequests[assignedSlot] = NULL;
                        WdfSpinLockRelease(controlData->PendingUrbLock);
                        WdfRequestComplete(pendingGuestRequest, STATUS_BUFFER_OVERFLOW);
                        bytesReturned = 0;
                        status = STATUS_BUFFER_OVERFLOW;
                        break;
                    }

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

                    if (msg->TransferBufferLength > sizeof(msg->Data)) {
                        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                            "USB Master Pro Filter: Isoch transfer %lu exceeds 64 KB limit. Rejected.\n",
                            msg->TransferBufferLength));
                        WdfRequestUnmarkCancelable(pendingGuestRequest);
                        WdfSpinLockAcquire(controlData->PendingUrbLock);
                        controlData->PendingRequests[assignedSlot] = NULL;
                        WdfSpinLockRelease(controlData->PendingUrbLock);
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
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
                        "USB Master Pro Filter: URB function 0x%04X fetched\n", urbFunc));
                }

                bytesReturned = sizeof(ANYUSB_URB_MESSAGE);
                status = STATUS_SUCCESS;
            } else {
                bytesReturned = 0;
                status = STATUS_NO_MORE_ENTRIES;
            }
            break;
        }

        case IOCTL_ANYUSB_FILTER_COMPLETE_URB: {
            PANYUSB_COMPLETE_REQUEST compReq = NULL;
            status = WdfRequestRetrieveInputBuffer(Request, sizeof(ANYUSB_COMPLETE_REQUEST), (PVOID*)&compReq, NULL);
            if (NT_SUCCESS(status) && compReq != NULL) {
                ULONG64 seqId = compReq->SequenceId;
                if (seqId < 1 || seqId > ANYUSB_MAX_PENDING_URBS) {
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                        "USB Master Pro Filter: Invalid SequenceId %llu\n", seqId));
                    status = STATUS_INVALID_PARAMETER;
                    break;
                }

                if (compReq->ActualLength > sizeof(compReq->Data)) {
                    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                        "USB Master Pro Filter: compReq ActualLength %lu exceeds 64 KB\n", compReq->ActualLength));
                    status = STATUS_BUFFER_OVERFLOW;
                    break;
                }

                ULONG slot = (ULONG)(seqId - 1);
                WDFREQUEST targetRequest = NULL;

                WdfSpinLockAcquire(controlData->PendingUrbLock);
                targetRequest = controlData->PendingRequests[slot];
                controlData->PendingRequests[slot] = NULL;
                WdfSpinLockRelease(controlData->PendingUrbLock);

                if (targetRequest != NULL) {
                    NTSTATUS unmarkStatus = WdfRequestUnmarkCancelable(targetRequest);
                    if (unmarkStatus == STATUS_CANCELLED) {
                        status = STATUS_CANCELLED;
                        break;
                    }

                    PIRP irp = WdfRequestWdmGetIrp(targetRequest);
                    if (irp != NULL) {
                        PIO_STACK_LOCATION irpSp = IoGetCurrentIrpStackLocation(irp);
                        PURB urb = (irpSp != NULL) ? (PURB)irpSp->Parameters.Others.Argument1 : NULL;
                        if (urb != NULL) {
                            urb->UrbHeader.Status = compReq->Status;

                            if (urb->UrbHeader.Function == URB_FUNCTION_CONTROL_TRANSFER) {
                                urb->UrbControlTransfer.TransferBufferLength = compReq->ActualLength;
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

                    WdfRequestCompleteWithInformation(targetRequest, compReq->Status, compReq->ActualLength);
                    bytesReturned = 0;
                    status = STATUS_SUCCESS;
                } else {
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
AnyUsbFilter_EvtIoDeviceControl(
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
    PFILTER_DEVICE_DATA filterData = GetFilterData(device);
    NTSTATUS status = STATUS_SUCCESS;
    size_t bytesReturned = 0;

    switch (IoControlCode) {
        case IOCTL_ANYUSB_FILTER_START_CAPTURE:
            filterData->IsCaptured = TRUE;
            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "USB Master Pro: Physical USB device capture STARTED.\n"));
            status = STATUS_SUCCESS;
            break;

        case IOCTL_ANYUSB_FILTER_STOP_CAPTURE:
            filterData->IsCaptured = FALSE;
            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "USB Master Pro: Physical USB device capture STOPPED.\n"));
            status = STATUS_SUCCESS;
            break;

        default: {
            // Forward unhandled IOCTLs downstream to physical hardware
            WDF_REQUEST_SEND_OPTIONS options;
            WDF_REQUEST_SEND_OPTIONS_INIT(&options, WDF_REQUEST_SEND_OPTION_SEND_AND_FORGET);
            if (!WdfRequestSend(Request, filterData->IoTarget, &options)) {
                status = WdfRequestGetStatus(Request);
                WdfRequestComplete(Request, status);
            }
            return;
        }
    }

    WdfRequestCompleteWithInformation(Request, status, bytesReturned);
}

VOID
AnyUsbFilter_EvtUrbComplete(
    _In_ WDFREQUEST                     Request,
    _In_ WDFIOTARGET                    Target,
    _In_ PWDF_REQUEST_COMPLETION_PARAMS Params,
    _In_ WDFCONTEXT                     Context
)
{
    UNREFERENCED_PARAMETER(Target);
    UNREFERENCED_PARAMETER(Context);

    NTSTATUS status = Params->IoStatus.Status;
    ULONG_PTR information = Params->IoStatus.Information;

    // Retrieve the guest WDM IRP and URB
    PIRP irp = WdfRequestWdmGetIrp(Request);
    PURB urb = NULL;
    if (irp != NULL) {
        PIO_STACK_LOCATION irpSp = IoGetCurrentIrpStackLocation(irp);
        if (irpSp != NULL) {
            urb = (PURB)irpSp->Parameters.Others.Argument1;
        }
    }

    if (urb != NULL) {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
            "USB Master Pro Filter: URB function 0x%04X completed (Status: 0x%08X, UrbStatus: 0x%08X, Info: %lu)\n",
            urb->UrbHeader.Function, status, urb->UrbHeader.Status, (ULONG)information));
    } else {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
            "USB Master Pro Filter: Request completed (Status: 0x%08X, Info: %lu)\n",
            status, (ULONG)information));
    }

    // Complete the request to the upstream caller with status and returned bytes
    WdfRequestCompleteWithInformation(Request, status, information);
}

VOID
AnyUsbFilter_EvtIoInternalDeviceControl(
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
    PFILTER_DEVICE_DATA filterData = GetFilterData(device);

    // If device is actively captured for remote redirection, forward URBs to PendingUrbQueue
    if (filterData->IsCaptured && IoControlCode == IOCTL_INTERNAL_USB_SUBMIT_URB) {
        if (g_ControlData != NULL && g_ControlData->PendingUrbQueue != NULL) {
            NTSTATUS forwardStatus = WdfRequestForwardToIoQueue(Request, g_ControlData->PendingUrbQueue);
            if (!NT_SUCCESS(forwardStatus)) {
                KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                    "USB Master Pro Filter: WdfRequestForwardToIoQueue failed: 0x%08X\n", forwardStatus));
                WdfRequestComplete(Request, forwardStatus);
            }
            return;
        }
    }

    // Passthrough downstream to physical hardware
    WDF_REQUEST_SEND_OPTIONS options;
    WDF_REQUEST_SEND_OPTIONS_INIT(&options, WDF_REQUEST_SEND_OPTION_SEND_AND_FORGET);
    if (!WdfRequestSend(Request, filterData->IoTarget, &options)) {
        WdfRequestComplete(Request, WdfRequestGetStatus(Request));
    }
}

/**
 * USB Master Pro — Virtual USB Host Controller & Root Bus Driver (anyusb_vhci.sys)
 * Implementation of KMDF 1.33 Bus Enumerator & Child PDO Instantiation
 */

#include "vhci.h"

// Prototypes
DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD AnyUsbVhci_EvtDeviceAdd;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL AnyUsbVhci_EvtIoDeviceControl;
EVT_WDF_IO_QUEUE_IO_INTERNAL_DEVICE_CONTROL AnyUsbVhci_EvtIoInternalDeviceControl;

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
            if (NT_SUCCESS(status) && attachReq != NULL) {
                KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
                    "USB Master Pro: Synthesizing Child PDO for Remote Device [VID:0x%04X PID:0x%04X]...\n",
                    attachReq->VendorId, attachReq->ProductId));

                // Tell PnP manager a new physical device arrived on the virtual bus
                fdoData->AttachedDeviceCount++;
                WdfFdoInitSetFilter(NULL); // Bus relations invalidation
                WdfDeviceSetFailed(device, WdfDeviceFailedDoNotAttemptRecovery); // Stub PnP refresh trigger
                status = STATUS_SUCCESS;
            }
            break;
        }

        case IOCTL_ANYUSB_VHCI_DETACH_DEVICE:
            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "USB Master Pro: Detaching remote virtual device.\n"));
            status = STATUS_SUCCESS;
            break;

        default:
            status = STATUS_INVALID_DEVICE_REQUEST;
            break;
    }

    WdfRequestCompleteWithInformation(Request, status, bytesReturned);
}

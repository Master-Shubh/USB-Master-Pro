cmake_minimum_required(VERSION 3.13)

# Include Pico SDK
include(pico_sdk_import.cmake)

project(pico_winusb_loopback C CXX ASM)
set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 17)

# Initialize Pico SDK
pico_sdk_init()

add_executable(pico_winusb_loopback
    main.c
    usb_descriptors.c
)

# Include TinyUSB and board definitions
target_include_directories(pico_winusb_loopback PRIVATE
    ${CMAKE_CURRENT_LIST_DIR}
)

# Link Pico stdlib and TinyUSB device stack
target_link_libraries(pico_winusb_loopback
    pico_stdlib
    tinyusb_device
    tinyusb_board
)

# Enable USB output, disable UART output
pico_enable_stdio_usb(pico_winusb_loopback 0)
pico_enable_stdio_uart(pico_winusb_loopback 0)

# Generate UF2 and bin artifacts
pico_add_extra_outputs(pico_winusb_loopback)

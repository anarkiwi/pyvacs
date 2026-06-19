; helloworld.asm - a "hello world" test pattern for the Emerson Arcadia 2001
; (and its clones, including the Australian Fountain Force 2), assembled with
; pyvacs and verified end-to-end on the MAME `arcadia` emulator.
;
; The program performs the minimal Signetics 2650 / Signetics 2637 UVI startup
; an Arcadia cartridge needs, then fills the whole screen with a single solid
; colour.  A solid colour is a deterministic, easy-to-check "test pattern": the
; integration test snapshots the emulator and asserts that essentially every
; pixel is the expected colour.
;
; The background colour is the low three bits of the register at $19F9:
;   $00 = white   $01 = yellow  $02 = cyan   $03 = green
;   $04 = magenta $05 = red     $06 = blue   $07 = black
;
; Build:    pyvacs-asm helloworld.asm -o helloworld.bin
; Run:      mame arcadia -cart helloworld.bin

BGCOLOUR        equ     $19F9       ; background colour (bits 0-2)
CHARCOLOUR      equ     $19FA       ; character colour register
CTRL            equ     $18FC       ; UVI control register
SCREEN_LO       equ     $1800       ; character/playfield RAM
SCREEN_MID      equ     $1900
SCREEN_HI       equ     $1A00
OBJ0Y           equ     $18F1       ; sprite Y positions (park them off-screen)
OBJ1Y           equ     $18F3
OBJ2Y           equ     $18F5

GREEN           equ     $03

                org     $0000

; The 2650 begins execution at $0000.  Real Arcadia carts jump past the small
; interrupt landing pad here, exactly as the original ROMs do.
reset:          eorz    r0                  ; zero R0
                bctr,un start
                retc,un                     ; (interrupt return pad)

start:          lpsu                        ; PSU = 0
                lpsl                        ; PSL = 0
                ppsu    $20                 ; set interrupt-inhibit
                ppsl    $02                 ; set the with-carry flag

                eorz    r0                  ; R0 = 0
                stra,r0 CTRL                ; reset the UVI control register

; Clear the three pages of video / object RAM ($1800, $1900, $1A00).
                strz    r1                  ; R1 = 0 (256-byte counter)
clrscr:         stra,r0 SCREEN_MID,r1
                stra,r0 SCREEN_LO,r1
                stra,r0 SCREEN_HI,r1
                bdrr,r1 clrscr              ; decrement R1, loop until it wraps to 0

; Park the hardware sprites off-screen so they cannot disturb the pattern.
                lodi,r0 $FF
                stra,r0 OBJ0Y
                stra,r0 OBJ1Y
                stra,r0 OBJ2Y

; Set the colours and paint the screen.
                lodi,r0 $07
                stra,r0 CHARCOLOUR
                lodi,r0 GREEN
                stra,r0 BGCOLOUR            ; whole screen becomes green

forever:        bctr,un forever            ; hold the pattern

                org     $07FF               ; pad the cartridge image to 2 KiB
                db      $00
                end

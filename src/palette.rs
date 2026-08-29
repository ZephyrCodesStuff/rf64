use crate::led::Color;
use avr_progmem::progmem;

#[cfg(feature = "dynamic-lighting")]
progmem! {
    pub static progmem ABLETON_COLORS: [Color; 128] = [
        Color::new(0, 0, 0),
        Color::new(43, 43, 43),
        Color::new(85, 85, 85),
        Color::new(128, 128, 128),
        Color::new(213, 64, 64),
        Color::new(255, 0, 0),
        Color::new(170, 0, 0),
        Color::new(85, 0, 0),
        Color::new(159, 117, 64),
        Color::new(223, 69, 0),
        Color::new(149, 48, 0),
        Color::new(74, 21, 0),
        Color::new(149, 149, 43),
        Color::new(159, 159, 0),
        Color::new(106, 106, 0),
        Color::new(53, 53, 0),
        Color::new(106, 202, 58),
        Color::new(69, 223, 0),
        Color::new(48, 149, 0),
        Color::new(21, 74, 0),
        Color::new(58, 213, 58),
        Color::new(0, 255, 0),
        Color::new(0, 170, 0),
        Color::new(0, 85, 0),
        Color::new(58, 202, 74),
        Color::new(0, 234, 21),
        Color::new(0, 154, 16),
        Color::new(0, 80, 11),
        Color::new(53, 191, 101),
        Color::new(32, 213, 74),
        Color::new(21, 138, 48),
        Color::new(11, 69, 21),
        Color::new(48, 170, 117),
        Color::new(0, 202, 117),
        Color::new(0, 133, 74),
        Color::new(0, 64, 37),
        Color::new(48, 128, 170),
        Color::new(0, 122, 191),
        Color::new(0, 80, 128),
        Color::new(0, 37, 64),
        Color::new(53, 101, 191),
        Color::new(0, 101, 207),
        Color::new(0, 69, 138),
        Color::new(0, 32, 69),
        Color::new(58, 58, 213),
        Color::new(0, 0, 255),
        Color::new(0, 0, 170),
        Color::new(0, 0, 85),
        Color::new(101, 53, 191),
        Color::new(101, 0, 207),
        Color::new(69, 0, 138),
        Color::new(32, 0, 69),
        Color::new(149, 43, 149),
        Color::new(159, 0, 159),
        Color::new(106, 0, 106),
        Color::new(53, 0, 53),
        Color::new(191, 53, 101),
        Color::new(223, 0, 69),
        Color::new(149, 0, 43),
        Color::new(74, 0, 21),
        Color::new(234, 16, 0),
        Color::new(149, 48, 0),
        Color::new(117, 80, 0),
        Color::new(64, 96, 0),
        Color::new(0, 53, 0),
        Color::new(0, 85, 48),
        Color::new(0, 80, 122),
        Color::new(0, 0, 255),
        Color::new(0, 64, 74),
        Color::new(32, 0, 202),
        Color::new(85, 85, 85),
        Color::new(43, 43, 43),
        Color::new(255, 0, 0),
        Color::new(122, 170, 27),
        Color::new(128, 175, 5),
        Color::new(80, 202, 5),
        Color::new(16, 138, 0),
        Color::new(0, 202, 106),
        Color::new(0, 133, 202),
        Color::new(0, 37, 234),
        Color::new(80, 0, 223),
        Color::new(101, 0, 213),
        Color::new(175, 21, 122),
        Color::new(64, 32, 0),
        Color::new(223, 64, 0),
        Color::new(117, 197, 5),
        Color::new(96, 213, 16),
        Color::new(0, 255, 0),
        Color::new(48, 223, 32),
        Color::new(64, 191, 80),
        Color::new(37, 170, 133),
        Color::new(69, 101, 191),
        Color::new(48, 80, 197),
        Color::new(101, 96, 175),
        Color::new(128, 16, 159),
        Color::new(213, 0, 80),
        Color::new(213, 80, 0),
        Color::new(181, 74, 0),
        Color::new(106, 191, 0),
        Color::new(128, 90, 5),
        Color::new(53, 43, 0),
        Color::new(16, 74, 16),
        Color::new(11, 80, 53),
        Color::new(16, 16, 37),
        Color::new(21, 32, 85),
        Color::new(101, 58, 27),
        Color::new(165, 0, 5),
        Color::new(197, 69, 53),
        Color::new(197, 96, 27),
        Color::new(159, 143, 27),
        Color::new(122, 175, 37),
        Color::new(101, 181, 16),
        Color::new(27, 27, 48),
        Color::new(128, 149, 64),
        Color::new(80, 159, 117),
        Color::new(101, 101, 170),
        Color::new(101, 74, 181),
        Color::new(32, 32, 32),
        Color::new(58, 58, 58),
        Color::new(112, 128, 128),
        Color::new(159, 0, 0),
        Color::new(48, 0, 0),
        Color::new(21, 207, 0),
        Color::new(5, 64, 0),
        Color::new(154, 149, 0),
        Color::new(58, 48, 0),
        Color::new(175, 90, 0),
        Color::new(74, 16, 0),
    ];
}

#[cfg(not(feature = "dynamic-lighting"))]
progmem! {
    /// Stock MIDI Fighter 64 pre-scaled palette from official C firmware (`display.c`).
    /// Values statically peak at <= 48 to stay under USB 500mA without dynamic scaling.
    pub static progmem ABLETON_COLORS: [Color; 128] = [
        Color::new(0, 0, 0),       // 0
        Color::new(8, 8, 8),       // 1
        Color::new(16, 16, 16),    // 2
        Color::new(24, 24, 24),    // 3
        Color::new(40, 12, 12),    // 4
        Color::new(48, 0, 0),      // 5
        Color::new(32, 0, 0),      // 6
        Color::new(16, 0, 0),      // 7
        Color::new(30, 22, 12),    // 8
        Color::new(42, 13, 0),     // 9
        Color::new(28, 9, 0),      // 10
        Color::new(14, 4, 0),      // 11
        Color::new(28, 28, 8),     // 12
        Color::new(30, 30, 0),     // 13
        Color::new(20, 20, 0),     // 14
        Color::new(10, 10, 0),     // 15
        Color::new(20, 38, 11),    // 16
        Color::new(13, 42, 0),     // 17
        Color::new(9, 28, 0),      // 18
        Color::new(4, 14, 0),      // 19
        Color::new(11, 40, 11),    // 20
        Color::new(0, 48, 0),      // 21
        Color::new(0, 32, 0),      // 22
        Color::new(0, 16, 0),      // 23
        Color::new(11, 38, 14),    // 24
        Color::new(0, 44, 4),      // 25
        Color::new(0, 29, 3),      // 26
        Color::new(0, 15, 2),      // 27
        Color::new(10, 36, 19),    // 28
        Color::new(6, 40, 14),     // 29
        Color::new(4, 26, 9),      // 30
        Color::new(2, 13, 4),      // 31
        Color::new(9, 32, 22),     // 32
        Color::new(0, 38, 22),     // 33
        Color::new(0, 25, 14),     // 34
        Color::new(0, 12, 7),      // 35
        Color::new(9, 24, 32),     // 36
        Color::new(0, 23, 36),     // 37
        Color::new(0, 15, 24),     // 38
        Color::new(0, 7, 12),      // 39
        Color::new(10, 19, 36),    // 40
        Color::new(0, 19, 39),     // 41
        Color::new(0, 13, 26),     // 42
        Color::new(0, 6, 13),      // 43
        Color::new(11, 11, 40),    // 44
        Color::new(0, 0, 48),      // 45
        Color::new(0, 0, 32),      // 46
        Color::new(0, 0, 16),      // 47
        Color::new(19, 10, 36),    // 48
        Color::new(19, 0, 39),     // 49
        Color::new(13, 0, 26),     // 50
        Color::new(6, 0, 13),      // 51
        Color::new(28, 8, 28),     // 52
        Color::new(30, 0, 30),     // 53
        Color::new(20, 0, 20),     // 54
        Color::new(10, 0, 10),     // 55
        Color::new(36, 10, 19),    // 56
        Color::new(42, 0, 13),     // 57
        Color::new(28, 0, 8),      // 58
        Color::new(14, 0, 4),      // 59
        Color::new(44, 3, 0),      // 60
        Color::new(28, 9, 0),      // 61
        Color::new(22, 15, 0),     // 62
        Color::new(12, 18, 0),     // 63
        Color::new(0, 10, 0),      // 64
        Color::new(0, 16, 9),      // 65
        Color::new(0, 15, 23),     // 66
        Color::new(0, 0, 48),      // 67
        Color::new(0, 12, 14),     // 68
        Color::new(6, 0, 38),      // 69
        Color::new(16, 16, 16),    // 70
        Color::new(8, 8, 8),       // 71
        Color::new(48, 0, 0),      // 72
        Color::new(23, 32, 5),     // 73
        Color::new(24, 33, 1),     // 74
        Color::new(15, 38, 1),     // 75
        Color::new(3, 26, 0),      // 76
        Color::new(0, 38, 20),     // 77
        Color::new(0, 25, 38),     // 78
        Color::new(0, 7, 44),      // 79
        Color::new(15, 0, 42),     // 80
        Color::new(19, 0, 40),     // 81
        Color::new(33, 4, 23),     // 82
        Color::new(12, 6, 0),      // 83
        Color::new(42, 12, 0),     // 84
        Color::new(22, 37, 1),     // 85
        Color::new(18, 40, 3),     // 86
        Color::new(0, 48, 0),      // 87
        Color::new(9, 42, 6),      // 88
        Color::new(12, 36, 15),    // 89
        Color::new(7, 32, 25),     // 90
        Color::new(13, 19, 36),    // 91
        Color::new(9, 15, 37),     // 92
        Color::new(19, 18, 33),    // 93
        Color::new(24, 3, 30),     // 94
        Color::new(40, 0, 15),     // 95
        Color::new(40, 15, 0),     // 96
        Color::new(34, 14, 0),     // 97
        Color::new(20, 36, 0),     // 98
        Color::new(24, 17, 1),     // 99
        Color::new(10, 8, 0),      // 100
        Color::new(3, 14, 3),      // 101
        Color::new(2, 15, 10),     // 102
        Color::new(3, 3, 7),       // 103
        Color::new(4, 6, 16),      // 104
        Color::new(19, 11, 5),     // 105
        Color::new(31, 0, 1),      // 106
        Color::new(37, 13, 10),    // 107
        Color::new(37, 18, 5),     // 108
        Color::new(30, 27, 5),     // 109
        Color::new(23, 33, 7),     // 110
        Color::new(19, 34, 3),     // 111
        Color::new(5, 5, 9),       // 112
        Color::new(24, 28, 12),    // 113
        Color::new(15, 30, 22),    // 114
        Color::new(19, 19, 32),    // 115
        Color::new(19, 14, 34),    // 116
        Color::new(6, 6, 6),       // 117
        Color::new(11, 11, 11),    // 118
        Color::new(21, 24, 24),    // 119
        Color::new(30, 0, 0),      // 120
        Color::new(9, 0, 0),       // 121
        Color::new(4, 39, 0),      // 122
        Color::new(1, 12, 0),      // 123
        Color::new(29, 28, 0),     // 124
        Color::new(11, 9, 0),      // 125
        Color::new(33, 17, 0),     // 126
        Color::new(14, 3, 0),      // 127
    ];
}

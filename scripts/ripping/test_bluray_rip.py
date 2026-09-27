import unittest
from pathlib import Path
from unittest.mock import patch

from bluray_rip import (
    build_parser,
    choose_title,
    conversion_command,
    parse_titles,
    robot_fields,
)


SAMPLE = '''TINFO:0,2,0,"Main Feature"
TINFO:0,9,0,"2:14:03"
TINFO:0,11,0,"73123456789"
TINFO:0,16,0,"00800.mpls"
TINFO:1,2,0,"Extras, Behind the Scenes"
TINFO:1,9,0,"0:12:10"
TINFO:1,11,0,"4123456789"
TINFO:1,16,0,"00801.mpls"'''


class ParsingTests(unittest.TestCase):
    def test_robot_fields_preserve_quoted_comma(self):
        self.assertEqual(robot_fields('1,2,0,"Extras, Behind the Scenes"')[3],
                         "Extras, Behind the Scenes")

    def test_parse_titles(self):
        titles = parse_titles(SAMPLE)
        self.assertEqual(len(titles), 2)
        self.assertEqual(titles[0].duration, "2:14:03")
        self.assertEqual(titles[0].playlist, "00800.mpls")
        self.assertEqual(titles[1].name, "Extras, Behind the Scenes")

    def test_largest_title_is_default(self):
        titles = parse_titles(SAMPLE)
        self.assertEqual(choose_title(titles, None).number, 0)
        self.assertEqual(choose_title(titles, 1).number, 1)

    @patch("bluray_rip.ffmpeg", return_value="/usr/local/bin/ffmpeg")
    def test_conversion_is_apple_compatible(self, _mock_ffmpeg):
        command = conversion_command(Path("in.mkv"), Path("out.mp4"), 20, False)
        self.assertIn("libx264", command)
        self.assertIn("yuv420p", command)
        self.assertIn("aac", command)
        self.assertIn("+faststart", command)
        self.assertEqual(command[-1], "out.mp4")

    @patch("bluray_rip.ffmpeg", return_value="/usr/local/bin/ffmpeg")
    def test_1080p_conversion_preserves_aspect_ratio(self, _mock_ffmpeg):
        command = conversion_command(
            Path("in.mkv"), Path("out.mp4"), 20, False, "1080p"
        )
        filter_value = command[command.index("-vf") + 1]
        self.assertIn("force_original_aspect_ratio=decrease", filter_value)
        self.assertIn("pad=1920:1080", filter_value)
        self.assertIn("setsar=1", filter_value)

    def test_scan_and_rip_default_to_two_second_minimum(self):
        parser = build_parser()
        self.assertEqual(parser.parse_args(["scan"]).min_length, 2)
        self.assertEqual(parser.parse_args(["rip", "output"]).min_length, 2)


if __name__ == "__main__":
    unittest.main()

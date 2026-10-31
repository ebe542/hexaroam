"""Ensure register-service host checks remain part of the common runner."""

import unittest

import dev


class RegisterTaskTests(unittest.TestCase):
    def test_register_service_is_included_in_each_relevant_task(self):
        expected = {
            "test": [["cargo", "test", "--lib", "--locked"]],
            "fmt": [["cargo", "fmt"]],
            "check": [["cargo", "fmt", "--check"], ["cargo", "check", "--locked"]],
            "verify": [["cargo", "fmt", "--check"], ["cargo", "test", "--lib", "--locked"]],
        }
        for task, commands in expected.items():
            with self.subTest(task=task):
                actual = [arguments for directory, arguments in dev.TASKS[task]
                          if directory == dev.REGISTERS_DIR]
                self.assertEqual(actual, commands)


if __name__ == "__main__":
    unittest.main()

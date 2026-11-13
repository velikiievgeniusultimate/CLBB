# SPDX-License-Identifier: GPL-3.0-or-later
"""Immutable deployment payloads cannot change an already loaded mapper inode."""
import importlib.util,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('live_mapper',ROOT/'tools/reload-mapper.py');live=importlib.util.module_from_spec(spec);spec.loader.exec_module(live)
class ImmutableMapperArtifacts(unittest.TestCase):
 def test_rebuild_publishes_new_inode_and_retains_previous_module(self):
  with tempfile.TemporaryDirectory()as temporary:
   root=Path(temporary);source=root/'build.so';source.write_bytes(b'\x7fELF'+b'A'*100)
   old,first=live.immutable_copy(source,root/'artifacts');inode=old.stat().st_ino
   source.write_bytes(b'\x7fELF'+b'B'*100);new,second=live.immutable_copy(source,root/'artifacts')
   self.assertNotEqual(first,second);self.assertNotEqual(old,new);self.assertEqual(old.stat().st_ino,inode)
   self.assertEqual(old.read_bytes(),b'\x7fELF'+b'A'*100);self.assertEqual(old.stat().st_mode&0o222,0)
   old.chmod(0o644);old.write_bytes(b'changed')
   source.write_bytes(b'\x7fELF'+b'A'*100)
   with self.assertRaisesRegex(ValueError,'Existing live artifact changed'):live.immutable_copy(source,root/'artifacts')
 def test_partial_and_non_elf_builds_never_publish(self):
  with tempfile.TemporaryDirectory()as temporary:
   root=Path(temporary);source=root/'build.so'
   for data in [b'',b'\x7fELF',b'broken'+b'A'*100]:
    source.write_bytes(data)
    with self.assertRaises(ValueError):live.immutable_copy(source,root/'artifacts')
   self.assertFalse((root/'artifacts').exists())

# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline socket-reply fixtures; no live mapper socket is opened."""
import importlib.util
import json
from pathlib import Path
import socket
import unittest
from unittest import mock

spec=importlib.util.spec_from_file_location('mapper_control',Path(__file__).resolve().parents[2]/'tools/mapper-control.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)


class ReplySocket:
    def __init__(self,chunks):self.chunks=list(chunks);self.timeouts=[];self.receives=0
    def settimeout(self,value):self.timeouts.append(value)
    def recv(self,limit):
        self.receives+=1
        if not self.chunks:return b''
        value=self.chunks.pop(0)
        if isinstance(value,Exception):raise value
        if len(value)>limit:self.chunks.insert(0,value[limit:]);value=value[:limit]
        return value


class MapperControlReplyTests(unittest.TestCase):
    def test_large_split_utf8_reply_is_complete_and_parseable(self):
        expected={'native_frame':'данные'*20000}
        raw=(json.dumps(expected,ensure_ascii=False)+'\n').encode()
        connection=ReplySocket([raw[i:i+997] for i in range(0,len(raw),997)])
        with mock.patch.object(module.time,'monotonic',return_value=0):
            reply=module.read_reply(connection,3.)
        self.assertEqual(json.loads(reply),expected)
        self.assertGreater(len(reply.encode()),4096)
        self.assertGreater(connection.receives,2)

    def test_reply_limit_is_inclusive_and_oversize_is_rejected(self):
        with mock.patch.object(module.time,'monotonic',return_value=0):
            self.assertEqual(module.read_reply(ReplySocket([b'12345678']),3.,max_bytes=8),'12345678')
            with self.assertRaisesRegex(ValueError,'exceeds'):
                module.read_reply(ReplySocket([b'123456789']),3.,max_bytes=8)

    def test_slow_drip_uses_one_global_deadline(self):
        connection=ReplySocket([b'a']*20)
        with mock.patch.object(module.time,'monotonic',side_effect=[0.,.9,1.8,2.7,3.6]):
            with self.assertRaisesRegex(TimeoutError,'deadline'):
                module.read_reply(connection,3.)
        self.assertEqual(connection.receives,4)
        self.assertLess(connection.timeouts[-1],.31)

    def test_timeout_and_malformed_utf8_remain_fatal(self):
        with mock.patch.object(module.time,'monotonic',return_value=0):
            with self.assertRaises(socket.timeout):
                module.read_reply(ReplySocket([socket.timeout('offline timeout')]),3.)
            with self.assertRaises(UnicodeDecodeError):
                module.read_reply(ReplySocket([b'\xff']),3.)

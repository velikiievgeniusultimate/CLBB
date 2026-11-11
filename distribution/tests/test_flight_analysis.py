# SPDX-License-Identifier: GPL-3.0-or-later
import importlib.util,json,tempfile,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('flight_analysis',Path(__file__).resolve().parents[2]/'tools/analyze-flight.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class FlightAnalysisTests(unittest.TestCase):
    def test_separates_transport_reader_and_core_rejections(self):
        header={'version':2,'start_ns':0,'end_ns':2_000_000_000,'truncated':False,'context':{'revision':'test'}}
        rows=[{'ns':100_000_000+i*1_000_000,'kind':'android_motion','data':{'packet':7,'stage':stage}}for i,stage in enumerate(['before_dispatch','after_dispatch','motion_event_reader'])]
        rows += [{'ns':200_000_000,'kind':'native_transport','data':{'accepted':False,'status':'mapper_cancel_shared_contacts_refused'}}, {'ns':210_000_000,'kind':'core_decision','data':{'event':'direction_feedback_rejected','reason':'sample_before_current_cast'}}]
        with tempfile.TemporaryDirectory()as directory:
            path=Path(directory)/'clip.jsonl';path.write_text('\n'.join(json.dumps(row)for row in [header,*rows]))
            result=module.analyze(path)
        self.assertEqual(result['packets_without_reader_in_window'],[])
        self.assertEqual(result['max_motion_reader_lag_ms'],2)
        self.assertEqual(result['transport_rejections'][0]['status'],'mapper_cancel_shared_contacts_refused')
        self.assertEqual(result['direction_rejections'],{'sample_before_current_cast':1})

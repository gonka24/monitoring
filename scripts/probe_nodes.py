#!/usr/bin/env python3
"""Read-only endpoint matrix. No response bodies or exception text in reports."""
import argparse
import base64
import concurrent.futures
import datetime
import json
import time
import urllib.error
import urllib.parse
import urllib.request

DEAL = 'gonka19q5apa9m2zu4qjytr22ga4trk49szya52cepk0p5lsgt520xypsqv252hy'
FACTORY = 'gonka1ea2s57dvs06fr5mjzmna9w9vt0xylrna5cjtmzr5h72vkrza3xxsp7eayy'


def smart(address, query):
    encoded = base64.b64encode(json.dumps(query, separators=(',', ':')).encode()).decode()
    return '/chain-api/cosmwasm/wasm/v1/contract/' + address + '/smart/' + urllib.parse.quote(encoded, safe='')


def probe(base, deal, timeout):
    url = urllib.parse.urlsplit(base)
    if url.scheme not in ('http', 'https') or not url.hostname or url.username or url.password or url.query or url.fragment or url.path not in ('', '/'):
        raise ValueError('Expected an HTTP(S) origin without credentials')
    rows = {}
    # Disable redirects, just like the service.
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            return None
    opener = urllib.request.build_opener(NoRedirect)

    def get(label, path):
        started = time.monotonic()
        body = None
        status = None
        reason = None
        try:
            with opener.open(base.rstrip('/') + path, timeout=timeout) as response:
                status = response.status
                body = json.load(response)
            if not isinstance(body, dict):
                reason = 'invalid_json_shape'
            elif body.get('error') or body.get('code', 0) not in (0, '0'):
                reason = 'rpc_error'
        except urllib.error.HTTPError as error:
            status = error.code
            reason = 'http_status'
        except (ValueError, OSError):
            reason = 'transport_or_decode'
        rows[label] = {'status': status, 'reason': reason, 'elapsed_ms': round((time.monotonic() - started) * 1000)}
        return body if reason is None else None

    status = get('status', '/chain-rpc/status')
    if status:
        result = status.get('result', {})
        sync = result.get('sync_info', {})
        rows['status']['chain_id'] = result.get('node_info', {}).get('network')
        rows['status']['tx_index'] = result.get('node_info', {}).get('other', {}).get('tx_index')
        rows['status']['catching_up'] = sync.get('catching_up')
        rows['status']['block_time'] = sync.get('latest_block_time')
    get('epoch', '/chain-api/productscience/inference/inference/get_current_epoch')
    get('contract_info', '/chain-api/cosmwasm/wasm/v1/contract/' + deal)
    config = get('config', smart(deal, {'config': {}}))
    get('state', smart(deal, {'state': {}}))
    get('registry', smart(FACTORY, {'list_deals': {'start_after': None, 'limit': 50}}))
    if config and isinstance(config.get('data'), dict) and config['data'].get('host'):
        host = config['data']['host']
        get('registration', smart(FACTORY, {'deal_by_host_epoch': {'host': host, 'epoch': config['data']['target_epoch']}}))
        get('recipient', '/chain-api/productscience/inference/inference/list_claim_recipients/' + urllib.parse.quote(host, safe=''))
    else:
        rows['registration'] = rows['recipient'] = {'status': None, 'reason': 'config_unavailable'}
    for label, query, order in [
        ('history', "execute._contract_address='" + deal + "'", 'asc'),
        ('search', "wasm-offer_created._contract_address='" + FACTORY + "'", 'desc'),
    ]:
        get(label, '/chain-rpc/tx_search?' + urllib.parse.urlencode({'query': json.dumps(query), 'page': '1', 'per_page': '20', 'order_by': json.dumps(order), 'prove': 'false'}))
    return {'node': base, 'checks': rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('config', help='JSON file with a nodes array')
    parser.add_argument('--deal', default=DEAL)
    parser.add_argument('--timeout', type=float, default=4)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    with open(args.config) as source:
        nodes = json.load(source)['nodes']
    if not 1 <= len(nodes) <= 64:
        parser.error('Expected 1 to 64 nodes')
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as executor:
        results = list(executor.map(lambda node: probe(node, args.deal, args.timeout), nodes))
    report = {'checked_at': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'source': 'local_probe', 'results': results}
    with open(args.output, 'w') as target:
        json.dump(report, target, indent=2)
    for result in results:
        print(result['node'], ' '.join(label + '=' + str(row['status'] or row['reason']) for label, row in result['checks'].items()))


if __name__ == '__main__':
    main()

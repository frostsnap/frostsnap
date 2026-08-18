# Real PSBTs

Produced by Bitcoin Core v31 on regtest, so the parser is pinned against a producer we do
not control rather than against PSBTs we construct ourselves. The wallet is the
deterministic test key `MasterAppkey::derive_from_rootkey(2 * G)` — fingerprint `4a340158`,
nothing private, nothing on mainnet.

To regenerate, import the key's descriptors into a watch-only Core wallet and let it fund
a transaction:

```
bitcoind -regtest -datadir=$DIR -fallbackfee=0.0001 -daemon
bitcoin-cli -regtest createwallet miner
bitcoin-cli -regtest generatetoaddress 101 $(bitcoin-cli -regtest -rpcwallet=miner getnewaddress)
bitcoin-cli -regtest createwallet fs true true "" false true true
bitcoin-cli -regtest -rpcwallet=fs importdescriptors '[
  {"desc":"tr([4a340158/0/0]tpubD6NzVbkrYhZ4Wyg7Bg8EFnVWzyUq8yKAtGKSFdSSRmiX2xPTkn4aAVNBK4owzJ38Sj2hAMq1DNyJhzApBiqEQ5LPfvXbNP2nMFaEEsqrLke/0/*)#fddnstpq",
   "timestamp":"now","active":true,"internal":false,"range":[0,50]},
  {"desc":"tr([4a340158/0/0]tpubD6NzVbkrYhZ4Wyg7Bg8EFnVWzyUq8yKAtGKSFdSSRmiX2xPTkn4aAVNBK4owzJ38Sj2hAMq1DNyJhzApBiqEQ5LPfvXbNP2nMFaEEsqrLke/1/*)#cegjd73c",
   "timestamp":"now","active":true,"internal":true,"range":[0,50]}]'
```

For `core_to_internal`, pay `getrawchangeaddress bech32m` so both outputs land on the
internal keychain.

Fund an address from `getnewaddress "" bech32m` (the default type is bech32 v0, which these
taproot descriptors cannot produce), then `walletcreatefundedpsbt` to a second address of
the same wallet for `core_selfsend`, or to the miner's for `core_stranger`.

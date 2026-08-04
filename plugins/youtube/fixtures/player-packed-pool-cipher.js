// Synthetic fixture matching the "packed constant-pool" player-JS shape that
// `cipher::extract_cipher_ops_packed_pool` parses. Not real YouTube player JS
// (which is large, not embeddable as a fixture, and reshapes itself often) —
// structurally faithful to what that strategy expects: a single `{`-delimited
// string standing in for a literal string array (discovered live against
// `youtube.player.web_20260729_10_RC00`, whose real pool held ~90 tokens this
// way instead of a quoted-string array literal), a 3-method helper object
// indexing into that pool by number instead of by name, and a dispatch call
// sequence whose XOR-obfuscated key indices are only resolvable once both the
// pool and the helper are known (see `solve_shared_base`'s doc comment).
//
// Token layout: splice@2, length@4, reverse@6 (read by the helper's own
// method bodies), sp@11 / sw@13 / rv@16 (the helper's own property names,
// read by the dispatch calls below). Dispatch base X=200; call constants
// were picked so X^195=11 ("sp"), X^216=16 ("rv"), X^197=13 ("sw"), and the
// swap call's own second argument X^205=5 is that operation's index.
//
// Expected extracted ops (source order): Splice(2), Reverse, Swap(5).
var POOL='t0{t1{splice{t3{length{t5{reverse{t7{t8{t9{t10{sp{t12{sw{t14{t15{rv{t17{t18{t19';
var HLP={
  sp:function(a,b){a[POOL[2]](0,b)},
  sw:function(a,b){var c=a[0];a[0]=a[b%a[POOL[4]]];a[b%a[POOL[4]]]=c},
  rv:function(a){a[POOL[6]]()}
};
xyz=function(a,X){a=a.split("");HLP[POOL[X^195]](a,2);HLP[POOL[X^216]](a);HLP[POOL[X^197]](a,X^205);return a.join("")};

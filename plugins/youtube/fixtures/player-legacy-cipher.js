// Synthetic fixture matching the "classic split/join" player-JS shape that
// `cipher::extract_cipher_ops`'s youtubeexplode-equivalent strategy parses.
// Not real YouTube player JS (which is large and not embeddable as a fixture)
// — structurally faithful to what the ported algorithm expects: a helper
// container object (`Bt`) whose methods are classified as Reverse/Splice/Swap
// by scanning their bodies, and a decipher function using
// `a=a.split("");CONTAINER.method(a,N);...;return a.join("")`.
//
// Expected extracted ops (source order): Reverse, Splice(2), Swap(61).
var Bt={
  aB:function(a,b){a.splice(0,b)},
  bC:function(a){a.reverse()},
  cD:function(a,b){var c=a[0];a[0]=a[b%a.length];a[b%a.length]=c}
};
xyz=function(a){a=a.split("");Bt.bC(a,3);Bt.aB(a,2);Bt.cD(a,61);return a.join("")};

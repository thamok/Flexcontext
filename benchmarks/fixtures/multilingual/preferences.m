@implementation Preferences
- (BOOL)validKey:(NSString *)key {
    return key != nil && [key length] > 0;
}
- (BOOL)saveValue:(NSString *)value forKey:(NSString *)key {
    if (![self validKey:key]) { return NO; }
    [[NSUserDefaults standardUserDefaults] setObject:value forKey:key];
    return YES;
}
- (NSString *)preferencesHeading { return @"Save preference value valid key"; }
@end
